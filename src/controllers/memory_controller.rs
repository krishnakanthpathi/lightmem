use crate::models::{
    detect_memory_conflict_with_similarity, GraphSnapshot, LightMemConfig, MemoryConflict,
    MemoryLink, MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories, RelatedMemory,
    ScoredMemory, StorageStats,
};
use crate::repositories::Storage;
use crate::services::{
    cosine_similarity, AnswerResult, EmbeddingProvider, Exporter, HashEmbeddingProvider,
    HybridSearchEngine, JsonMemoryImporter, MemoryImporter, OkfMemoryImporter,
    OllamaEmbeddingProvider, OllamaReranker, OnnxEmbeddingProvider, OnnxQaReranker, Reranker,
    Top1Reranker,
};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub struct LightMem {
    storage: Arc<Storage>,
    embedder: OnceLock<Arc<dyn EmbeddingProvider>>,
    config: LightMemConfig,
    embedding_identity: String,
}

pub type MemoryController = LightMem;

impl LightMem {
    /// Open the active database based on configuration (local project or global)
    pub fn open_default(force_global: bool) -> Result<Self> {
        let config = LightMemConfig::try_load()?;
        let db_path = LightMemConfig::resolve_db_path(force_global);
        Self::open_at(&db_path, config)
    }

    /// Open database at a specific path (embedding model is loaded lazily on first use)
    pub fn open_at(db_path: &Path, config: LightMemConfig) -> Result<Self> {
        let embedding_identity = config.embedding_identity()?;
        let storage = Storage::open(db_path)?;

        Ok(Self {
            storage: Arc::new(storage),
            embedder: OnceLock::new(),
            config,
            embedding_identity,
        })
    }

    fn embedder(&self) -> Result<&Arc<dyn EmbeddingProvider>> {
        if let Some(emb) = self.embedder.get() {
            return Ok(emb);
        }

        let created: Arc<dyn EmbeddingProvider> = match self.config.backend.as_str() {
            "onnx" => {
                let model_str = self.config.onnx_model.as_deref().unwrap_or("bge-small");
                let path = std::path::Path::new(model_str);
                if path.is_dir() {
                    Arc::new(OnnxEmbeddingProvider::new_custom_dir(path)?)
                } else {
                    Arc::new(OnnxEmbeddingProvider::new(Some(model_str))?)
                }
            }
            "ollama" => Arc::new(OllamaEmbeddingProvider::new(
                self.config.ollama_url.clone(),
                self.config.embedding_model.clone(),
            )),
            "hash" => Arc::new(HashEmbeddingProvider),
            other => anyhow::bail!("Unknown embedding backend: {}", other),
        };

        let _ = self.embedder.set(created);
        Ok(self.embedder.get().unwrap())
    }

    pub fn embedding_migration_needed(&self) -> Result<bool> {
        Ok(self.storage.count_memories(None, None, None)? > 0
            && self.storage.embedding_identity()?.as_deref() != Some(&self.embedding_identity))
    }

    pub fn stored_embedding_identity(&self) -> Result<Option<String>> {
        self.storage.embedding_identity()
    }

    pub fn requested_embedding_identity(&self) -> &str {
        &self.embedding_identity
    }

    /// Build a replacement index off to the side. Publish all vectors together only if
    /// no memories/index changed in the meantime. Failures leave the old index intact.
    pub fn reindex(&self, mut progress: impl FnMut(usize, usize)) -> Result<usize> {
        let (revision, memories) = self.storage.index_snapshot()?;
        let total = memories.len();
        progress(0, total);
        let mut vectors = Vec::with_capacity(total);
        if total > 0 {
            let embedder = self.embedder()?;
            for batch in memories.chunks(32) {
                let texts: Vec<String> = batch.iter().map(MemoryRecord::to_card_text).collect();
                let embeddings = embedder.embed_batch(&texts)?;
                anyhow::ensure!(
                    embeddings.len() == batch.len(),
                    "Embedding provider returned the wrong batch size"
                );
                for (memory, vector) in batch.iter().zip(embeddings) {
                    crate::embeddings::validate_vector(&vector)?;
                    vectors.push((memory.id.clone(), vector));
                }
                progress(vectors.len(), total);
            }
        }
        self.storage
            .replace_index(revision, &self.embedding_identity, &vectors)?;
        Ok(total)
    }

    pub fn db_path(&self) -> &Path {
        self.storage.path()
    }

    pub fn config(&self) -> &LightMemConfig {
        &self.config
    }

    /// Store a new memory into the database
    pub fn remember(
        &self,
        content: &str,
        category: Option<MemoryType>,
        title: Option<String>,
        tags: Vec<String>,
        confidence: Option<f32>,
    ) -> Result<MemoryRecord> {
        let (record, _) =
            self.remember_with_options(content, category, title, tags, confidence, None, false)?;
        Ok(record)
    }

    /// Store a new memory into the database with optional TTL and contradiction superseding
    #[allow(clippy::too_many_arguments)]
    pub fn remember_with_options(
        &self,
        content: &str,
        category: Option<MemoryType>,
        title: Option<String>,
        tags: Vec<String>,
        confidence: Option<f32>,
        ttl: Option<chrono::Duration>,
        supersede: bool,
    ) -> Result<(MemoryRecord, Vec<MemoryConflict>)> {
        let clean_content = content.trim().to_string();
        if clean_content.is_empty() {
            anyhow::bail!("Memory content cannot be blank");
        }

        let resolved_type = category.unwrap_or_else(|| MemoryType::infer(&clean_content));
        let resolved_title = title.unwrap_or_else(|| {
            clean_content
                .lines()
                .next()
                .unwrap_or("Untitled Memory")
                .chars()
                .take(80)
                .collect()
        });

        let conf = confidence.unwrap_or(0.9);
        anyhow::ensure!(
            conf.is_finite() && (0.0..=1.0).contains(&conf),
            "Confidence must be between 0 and 1"
        );
        let mut memory = MemoryRecord::new(
            resolved_type,
            resolved_title,
            clean_content,
            tags,
            conf,
            Some("explicit_statement".to_string()),
        );
        if let Some(ttl_duration) = ttl {
            memory.expired_at = Some(Utc::now() + ttl_duration);
        }

        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let mut stored_batch = self.storage.insert_indexed_batch(
            &[memory],
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            true,
        )?;
        let mut stored = stored_batch.remove(0);

        let active_peers = self.storage.list_memories(
            Some(stored.category),
            Some(MemoryStatus::Active),
            None,
            0,
        )?;
        let vec_map: HashMap<String, Vec<f32>> = self
            .storage
            .get_candidate_vectors_checked(
                Some(stored.category),
                Some(MemoryStatus::Active),
                None,
                Some(&self.embedding_identity),
            )
            .unwrap_or_default()
            .into_iter()
            .collect();

        let stored_vec = vec_map.get(&stored.id);
        let mut best_conflict: Option<MemoryConflict> = None;

        for peer in &active_peers {
            if peer.id == stored.id {
                continue;
            }
            let vec_sim =
                stored_vec.and_then(|sv| vec_map.get(&peer.id).map(|pv| cosine_similarity(sv, pv)));
            if let Some(conflict) =
                detect_memory_conflict_with_similarity(&stored, peer, vec_sim, 0.72)
            {
                if conflict.older_memory.id == peer.id
                    && best_conflict
                        .as_ref()
                        .map(|prev| conflict.similarity > prev.similarity)
                        .unwrap_or(true)
                {
                    best_conflict = Some(conflict);
                }
            }
        }

        let mut conflicts = Vec::new();
        if let Some(conflict) = best_conflict {
            if supersede {
                stored = self.merge_conflict_pair(&conflict, None)?;
            }
            conflicts.push(conflict);
        }

        // Auto-link any wikilinks [[...]] in memory content and title
        let _ = self.storage.auto_link_memory(
            &stored.id,
            &format!("{}\n{}", stored.title, stored.content),
        );

        Ok((stored, conflicts))
    }

    /// Scan active memories by finding each memory's closest vector neighbor and checking overlap.
    pub fn find_conflicts(&self, resolve: bool) -> Result<Vec<MemoryConflict>> {
        self.find_conflicts_with_options(resolve, 0.78, None)
    }

    /// Nearest-neighbor conflict & overlap detector with configurable similarity threshold and reranker merge
    pub fn find_conflicts_with_options(
        &self,
        resolve: bool,
        min_similarity: f32,
        reranker_override: Option<&str>,
    ) -> Result<Vec<MemoryConflict>> {
        let active = self
            .storage
            .list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let vec_map: HashMap<String, Vec<f32>> = self
            .storage
            .get_candidate_vectors_checked(None, Some(MemoryStatus::Active), None, None)
            .unwrap_or_default()
            .into_iter()
            .collect();

        let mut seen_pairs = HashSet::new();
        let mut conflicts = Vec::new();

        for i in 0..active.len() {
            let mem_i = &active[i];
            let vec_i = vec_map.get(&mem_i.id);
            let mut closest_conflict: Option<MemoryConflict> = None;

            for (j, mem_j) in active.iter().enumerate() {
                if i == j {
                    continue;
                }
                let vec_sim =
                    vec_i.and_then(|vi| vec_map.get(&mem_j.id).map(|vj| cosine_similarity(vi, vj)));
                if let Some(c) =
                    detect_memory_conflict_with_similarity(mem_i, mem_j, vec_sim, min_similarity)
                {
                    if closest_conflict
                        .as_ref()
                        .map(|prev| c.similarity > prev.similarity)
                        .unwrap_or(true)
                    {
                        closest_conflict = Some(c);
                    }
                }
            }

            if let Some(conflict) = closest_conflict {
                let pair_key = (
                    conflict.older_memory.id.clone(),
                    conflict.newer_memory.id.clone(),
                );
                if seen_pairs.insert(pair_key) {
                    conflicts.push(conflict);
                }
            }
        }

        conflicts.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if resolve {
            let mut retired_ids = HashSet::new();
            let mut resolved = Vec::new();
            for conflict in conflicts {
                if retired_ids.contains(&conflict.older_memory.id)
                    || retired_ids.contains(&conflict.newer_memory.id)
                {
                    continue;
                }
                let _ = self.merge_conflict_pair(&conflict, reranker_override)?;
                retired_ids.insert(conflict.older_memory.id.clone());
                resolved.push(conflict);
            }
            return Ok(resolved);
        }

        Ok(conflicts)
    }

    /// Merge a single conflict pair using the active (or overridden) Reranker, re-indexing the survivor and soft-retiring the older record.
    pub fn merge_conflict_pair(
        &self,
        conflict: &MemoryConflict,
        reranker_override: Option<&str>,
    ) -> Result<MemoryRecord> {
        let older = self
            .storage
            .get_memory(&conflict.older_memory.id)?
            .unwrap_or_else(|| conflict.older_memory.clone());
        let newer = self
            .storage
            .get_memory(&conflict.newer_memory.id)?
            .unwrap_or_else(|| conflict.newer_memory.clone());
        let reranker = self.build_reranker(reranker_override)?;
        let merged = reranker.merge_conflict(&older, &newer)?;
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let mut updated = self.storage.insert_indexed_batch(
            &[merged],
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            false,
        )?;
        let _ = self.storage.forget_memory(&older.id, false)?;
        Ok(updated.remove(0))
    }

    /// Semantic + BM25 Hybrid Recall
    pub fn recall(
        &self,
        query: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        min_similarity: Option<f32>,
    ) -> Result<Vec<ScoredMemory>> {
        self.recall_with_date(query, category, as_of, None, limit, min_similarity)
    }

    /// Semantic + BM25 Hybrid Recall with optional exact calendar day filter (`--date YYYY-MM-DD`)
    pub fn recall_with_date(
        &self,
        query: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        limit: usize,
        min_similarity: Option<f32>,
    ) -> Result<Vec<ScoredMemory>> {
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let fetch_limit = if on_date.is_some() {
            limit.max(250)
        } else {
            limit
        };
        let mut results = HybridSearchEngine::search_with_identity(
            &self.storage,
            self.embedder()?.as_ref(),
            query,
            category,
            Some(MemoryStatus::Active),
            as_of,
            fetch_limit,
            min_similarity,
            Some(&self.embedding_identity),
        )?;
        if let Some(target_day) = on_date {
            results.retain(|r| {
                r.memory.created_at.date_naive() == target_day
                    || r.memory.updated_at.date_naive() == target_day
            });
            results.truncate(limit);
        }
        Ok(results)
    }

    /// Hybrid Recall with optional multi-hop expansion along knowledge graph links
    pub fn recall_expanded(
        &self,
        query: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        limit: usize,
        min_similarity: Option<f32>,
        multi_hop: bool,
    ) -> Result<Vec<ScoredMemory>> {
        let base_results =
            self.recall_with_date(query, category, as_of, on_date, limit, min_similarity)?;
        if !multi_hop || base_results.is_empty() {
            return Ok(base_results);
        }

        let mut seen_ids: HashSet<String> =
            base_results.iter().map(|r| r.memory.id.clone()).collect();
        let mut expanded = base_results;

        let top_candidates: Vec<(String, f32)> = expanded[..expanded.len().min(3)]
            .iter()
            .map(|r| (r.memory.id.clone(), r.score))
            .collect();

        for (id, score) in top_candidates {
            if let Ok(related) = self.storage.traverse_multi_hop(&id, 1) {
                for rel in related {
                    if seen_ids.insert(rel.memory.id.clone()) {
                        expanded.push(ScoredMemory {
                            memory: rel.memory,
                            score: score * 0.8,
                            bm25_rank: None,
                            vector_rank: None,
                        });
                    }
                }
            }
        }
        Ok(expanded)
    }

    /// Count total memories matching optional filters
    pub fn count(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<usize> {
        self.storage.count_memories(category, status, as_of)
    }

    /// List memories with pagination (limit + offset) and total metadata
    pub fn list_paginated(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedMemories> {
        self.list_paginated_with_date(category, status, as_of, None, limit, offset)
    }

    pub fn list_paginated_with_date(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedMemories> {
        let Some(target_day) = on_date else {
            return self
                .storage
                .list_memories_paginated(category, status, as_of, limit, offset);
        };
        let all = self.storage.list_memories(category, status, as_of, 0)?;
        let filtered: Vec<MemoryRecord> = all
            .into_iter()
            .filter(|m| {
                m.created_at.date_naive() == target_day || m.updated_at.date_naive() == target_day
            })
            .collect();
        let total = filtered.len();
        let items: Vec<MemoryRecord> = filtered.into_iter().skip(offset).take(limit).collect();
        let page = offset.checked_div(limit).unwrap_or(0) + 1;
        let total_pages = if limit > 0 { total.div_ceil(limit) } else { 1 };
        let has_more = offset.saturating_add(items.len()) < total;
        Ok(PaginatedMemories {
            items,
            total,
            limit,
            offset,
            page,
            total_pages,
            has_more,
        })
    }

    /// List memories by 1-indexed page number and page size
    pub fn list_page(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        page: usize,
        per_page: usize,
    ) -> Result<PaginatedMemories> {
        self.list_page_with_date(category, status, as_of, None, page, per_page)
    }

    pub fn list_page_with_date(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        page: usize,
        per_page: usize,
    ) -> Result<PaginatedMemories> {
        anyhow::ensure!(
            page > 0 && per_page > 0,
            "Page and page size must be positive"
        );
        let offset = (page - 1)
            .checked_mul(per_page)
            .context("Pagination offset is too large")?;
        self.list_paginated_with_date(category, status, as_of, on_date, per_page, offset)
    }

    /// List memories with optional filtering
    pub fn list(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        self.storage.list_memories(category, status, as_of, limit)
    }

    /// Retrieve single memory by ID
    pub fn get(&self, id: &str) -> Result<Option<MemoryRecord>> {
        self.storage.get_memory(id)
    }

    /// Forget (soft-delete or hard delete) a memory
    pub fn forget(&self, id: &str, hard_delete: bool) -> Result<bool> {
        self.storage.forget_memory(id, hard_delete)
    }

    /// Clear (soft-retire or permanently delete) all memories in the database
    pub fn clear_all(&self, hard_delete: bool) -> Result<usize> {
        self.storage.clear_all(hard_delete)
    }

    /// Scan active memories, smart-merge duplicate content (union tags, max confidence, best title, earliest created_at), and delete redundant rows
    pub fn deduplicate(&self) -> Result<usize> {
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        self.storage
            .deduplicate_indexed(self.embedder()?.as_ref(), &self.embedding_identity)
    }

    /// Explicitly link two memories with an optional relation type (default: relates_to) and weight
    pub fn link(
        &self,
        source: &str,
        target: &str,
        relation: Option<&str>,
        weight: Option<f32>,
    ) -> Result<(String, String)> {
        self.storage.add_link(source, target, relation, weight)
    }

    /// Remove explicit connection between two memories
    pub fn unlink(&self, source: &str, target: &str, relation: Option<&str>) -> Result<bool> {
        self.storage.remove_link(source, target, relation)
    }

    /// Retrieve all direct outgoing and incoming links for a memory
    pub fn get_links(&self, memory_id: &str) -> Result<Vec<MemoryLink>> {
        self.storage.get_links_for_memory(memory_id)
    }

    /// Automatically discover and connect memories via title mentions, shared tags, and vector similarity
    pub fn autolink(&self, min_similarity: Option<f32>) -> Result<usize> {
        let threshold = min_similarity.unwrap_or(0.75).clamp(0.1, 1.0);
        self.storage.autolink_vault(threshold)
    }

    /// Traverse knowledge graph up to N hops from a starting memory
    pub fn related(&self, memory_id: &str, hops: usize) -> Result<Vec<RelatedMemory>> {
        self.storage.traverse_multi_hop(memory_id, hops)
    }

    /// Get knowledge graph snapshot (all active nodes and edges, or focused neighborhood)
    pub fn graph(
        &self,
        focus: Option<&str>,
        hops: Option<usize>,
    ) -> Result<GraphSnapshot> {
        self.storage.get_graph_snapshot(focus, hops)
    }


    /// Export memories to an OKF bundle
    pub fn export_okf(&self, target_path: Option<&Path>) -> Result<PathBuf> {
        Exporter::export_okf(&self.storage, target_path)
    }

    pub fn export_json(&self, target_path: &Path) -> Result<PathBuf> {
        Exporter::export_json(&self.storage, target_path)
    }

    /// Import memories from an external file (.json, .jsonl, or .md/.okf) or an OKF bundle directory
    pub fn import_file(&self, file_path: &Path) -> Result<usize> {
        self.import_file_with_enrichment(file_path, false)
    }

    /// Parse every file before changing the database; one failed record rolls back the entire import.
    pub fn import_file_with_enrichment(&self, file_path: &Path, enrich: bool) -> Result<usize> {
        let mut files = Vec::new();
        if file_path.is_dir() {
            let root = if file_path.join("memories").is_dir() {
                file_path.join("memories")
            } else {
                file_path.to_path_buf()
            };
            Self::collect_importable_files(&root, &mut files)?;
            files.sort();
        } else {
            files.push(file_path.to_path_buf());
        }
        let mut records = Vec::new();
        let mut restore = false;
        for file in files {
            let raw = std::fs::read_to_string(&file)
                .with_context(|| format!("Failed to read {}", file.display()))?;
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            let json = ext.eq_ignore_ascii_case("json")
                || ext.eq_ignore_ascii_case("jsonl")
                || raw.trim_start().starts_with(['{', '[']);
            let candidates = if json {
                JsonMemoryImporter.parse_with_enrichment(&raw, enrich)?
            } else {
                OkfMemoryImporter.parse(&raw)?
            };
            restore |= candidates.iter().any(|c| c.status.is_some());
            for candidate in candidates {
                anyhow::ensure!(
                    candidate.confidence.is_finite() && (0.0..=1.0).contains(&candidate.confidence),
                    "Invalid import confidence"
                );
                records.push(candidate.to_memory_record());
            }
        }
        if records.is_empty() {
            return Ok(0);
        }
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        self.storage.insert_indexed_batch(
            &records,
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            !restore,
        )?;
        Ok(records.len())
    }

    fn collect_importable_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in
            std::fs::read_dir(dir).with_context(|| format!("Failed to read directory {:?}", dir))?
        {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_symlink() {
                continue;
            }
            if path.is_dir() {
                Self::collect_importable_files(&path, out)?;
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.eq_ignore_ascii_case("index.md") || name.starts_with('.') {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext.eq_ignore_ascii_case("md")
                    || ext.eq_ignore_ascii_case("okf")
                    || ext.eq_ignore_ascii_case("json")
                    || ext.eq_ignore_ascii_case("jsonl")
                {
                    out.push(path);
                }
            }
        }
        Ok(())
    }

    /// Get database statistics
    pub fn stats(&self) -> Result<StorageStats> {
        self.storage.stats()
    }

    pub fn build_reranker(&self, reranker_override: Option<&str>) -> Result<Box<dyn Reranker>> {
        let active_reranker = reranker_override.unwrap_or(&self.config.reranker).trim();
        let lower = active_reranker.to_lowercase();

        if lower == "top1" {
            return Ok(Box::new(Top1Reranker));
        }

        if lower == "ollama" {
            let cfg_ollama = self
                .config
                .reranker
                .strip_prefix("ollama:")
                .or_else(|| self.config.reranker.strip_prefix("OLLAMA:"))
                .map(|s| s.to_string());
            return Ok(Box::new(OllamaReranker::new(
                self.config.ollama_url.clone(),
                cfg_ollama,
            )));
        }

        if let Some(ollama_model) = active_reranker
            .strip_prefix("ollama:")
            .or_else(|| active_reranker.strip_prefix("OLLAMA:"))
        {
            return Ok(Box::new(OllamaReranker::new(
                self.config.ollama_url.clone(),
                Some(ollama_model.to_string()),
            )));
        }

        if let Some(onnx_model) = active_reranker
            .strip_prefix("onnx:")
            .or_else(|| active_reranker.strip_prefix("ONNX:"))
        {
            return Ok(Box::new(OnnxQaReranker::new(Some(onnx_model.to_string()))));
        }

        match lower.as_str() {
            "onnx" | "qa" | "precision" => {
                let default_onnx = if matches!(
                    self.config.reranker.to_lowercase().as_str(),
                    "tinyroberta-squad2" | "tinyroberta"
                ) {
                    "tinyroberta-squad2"
                } else {
                    "minilm-squad2"
                };
                Ok(Box::new(OnnxQaReranker::new(Some(default_onnx.to_string()))))
            }
            "minilm-squad2" | "minilm" | "tinyroberta-squad2" | "tinyroberta" => {
                Ok(Box::new(OnnxQaReranker::new(Some(lower))))
            }
            other => anyhow::bail!(
                "Unknown reranker: '{}'. Choose 'minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama' ('ollama:<model>'), or 'top1'.",
                other
            ),
        }
    }

    /// Answer a natural language question using retrieved memory candidates
    /// and either Top-1 direct selection (0ms) or Extractive QA / Ollama reranking.
    pub fn answer(
        &self,
        question: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        use_precision: bool,
    ) -> Result<AnswerResult> {
        let override_reranker = if use_precision {
            if self.config.reranker.eq_ignore_ascii_case("top1") {
                Some("onnx")
            } else {
                Some(self.config.reranker.as_str())
            }
        } else {
            None
        };
        self.answer_with_options(question, category, as_of, None, limit, override_reranker)
    }

    /// Answer with an explicit per-query reranker override ("top1", "onnx", "onnx:<model>", "ollama", "ollama:<model>")
    pub fn answer_with_reranker(
        &self,
        question: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        reranker_override: Option<&str>,
    ) -> Result<AnswerResult> {
        self.answer_with_options(question, category, as_of, None, limit, reranker_override)
    }

    pub fn answer_with_options(
        &self,
        question: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        limit: usize,
        reranker_override: Option<&str>,
    ) -> Result<AnswerResult> {
        let candidates = self.recall_with_date(question, category, as_of, on_date, limit, None)?;
        let reranker = self.build_reranker(reranker_override)?;
        reranker.answer(question, &candidates)
    }
}
