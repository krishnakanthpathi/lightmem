use crate::models::{
    compute_sha256_hex, detect_memory_conflict_with_similarity, AttachmentInput,
    AttachmentMetadata, GraphSnapshot, LightMemConfig, MemoryConflict, MemoryLink, MemoryRecord,
    MemoryStatus, MemoryType, PaginatedMemories, RelatedMemory, ScoredMemory, StorageStats,
};
use crate::repositories::Storage;
use crate::services::{
    cosine_similarity, extract_imported_links, AnswerResult, EmbeddingProvider, Exporter,
    HashEmbeddingProvider, HybridSearchEngine, JsonMemoryImporter, MemoryImporter,
    ObservedCandidate, ObserverService, OkfMemoryImporter, OllamaEmbeddingProvider, OllamaReranker,
    OnnxEmbeddingProvider, OnnxQaReranker, Reranker, Top1Reranker,
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

    /// Open one registered scope. Every storage operation uses the selected vault.
    pub fn open_connection(
        id: &str,
        scope: Option<crate::models::MemoryScope>,
        session: Option<String>,
    ) -> Result<Self> {
        let resolved =
            crate::models::IdentityRegistry::open_default()?.resolve(id, scope, session)?;
        Self::open_resolved(&resolved)
    }

    pub fn open_resolved(connection: &crate::models::ResolvedConnection) -> Result<Self> {
        let config = LightMemConfig::try_load()?;
        let embedding_identity = config.embedding_identity()?;
        let storage = Storage::open(&connection.db_path)?.with_context(connection.context.clone());
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
            "ollama" => Arc::new(OllamaEmbeddingProvider::with_api_key(
                self.config.effective_ollama_url(),
                self.config.embedding_model.clone(),
                self.config.effective_ollama_api_key(),
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
                let embeddings = if embedder.supports_multimodal() {
                    let mut media_payloads = Vec::with_capacity(batch.len());
                    for rec in batch {
                        let media = if !rec.attachments.is_empty() {
                            self.storage.primary_multimodal_payload(&rec.id)?
                        } else {
                            None
                        };
                        media_payloads.push(media);
                    }
                    let inputs: Vec<crate::services::embeddings::MultimodalInput> = texts
                        .iter()
                        .zip(&media_payloads)
                        .map(|(text, media)| match media.as_ref() {
                            Some((Some(img), _)) => {
                                crate::services::embeddings::MultimodalInput::with_image(
                                    text,
                                    img.as_str(),
                                )
                            }
                            Some((_, Some(aud))) => {
                                crate::services::embeddings::MultimodalInput::with_audio(
                                    text,
                                    aud.as_str(),
                                )
                            }
                            _ => crate::services::embeddings::MultimodalInput::text_only(text),
                        })
                        .collect();
                    embedder.embed_multimodal_batch(&inputs)?
                } else {
                    embedder.embed_batch(&texts)?
                };
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
        let (record, _) = self.remember_with_options(
            content, category, title, tags, confidence, None, false, None,
        )?;
        Ok(record)
    }

    /// Store a new memory into the database with optional TTL, contradiction superseding, and provenance
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
        provenance: Option<String>,
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
        let clean_provenance = provenance
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "explicit_statement".to_string());
        let mut memory = MemoryRecord::new(
            resolved_type,
            resolved_title,
            clean_content,
            tags,
            conf,
            Some(clean_provenance),
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
        let _ = self
            .storage
            .auto_link_memory(&stored.id, &format!("{}\n{}", stored.title, stored.content));

        Ok((stored, conflicts))
    }

    /// Store a memory with one or more file/image/audio/document attachments.
    /// By default (`extract_llm == false`), only deterministic file metadata is attached and indexed.
    /// When `extract_llm == true`, the single configured LLM is invoked to extract text/descriptions.
    #[allow(clippy::too_many_arguments)]
    pub fn remember_with_attachments(
        &self,
        content: Option<&str>,
        files: &[PathBuf],
        extract_llm: bool,
        reranker_override: Option<&str>,
        category: Option<MemoryType>,
        title: Option<String>,
        tags: Vec<String>,
        confidence: Option<f32>,
        ttl: Option<chrono::Duration>,
        supersede: bool,
        provenance: Option<String>,
    ) -> Result<(MemoryRecord, Vec<MemoryConflict>)> {
        let (stored, conflicts, _) = self.remember_with_attachments_and_entities(
            content,
            files,
            extract_llm,
            false,
            reranker_override,
            category,
            title,
            tags,
            confidence,
            ttl,
            supersede,
            provenance,
        )?;
        Ok((stored, conflicts))
    }

    /// Store a memory with optional file attachments and optional LLM entity extraction.
    /// Returns the stored record, detected conflicts, and an optional warning if entity extraction failed or duplicates were found.
    #[allow(clippy::too_many_arguments)]
    pub fn remember_with_attachments_and_entities(
        &self,
        content: Option<&str>,
        files: &[PathBuf],
        extract_llm: bool,
        extract_entities: bool,
        reranker_override: Option<&str>,
        category: Option<MemoryType>,
        title: Option<String>,
        mut tags: Vec<String>,
        confidence: Option<f32>,
        ttl: Option<chrono::Duration>,
        supersede: bool,
        provenance: Option<String>,
    ) -> Result<(MemoryRecord, Vec<MemoryConflict>, Option<String>)> {
        let mut attachment_inputs = Vec::with_capacity(files.len());
        let mut seen_batch_sha = HashSet::new();
        let mut warnings = Vec::new();
        for path in files {
            let att = AttachmentInput::from_path(path)?;
            if !seen_batch_sha.insert(att.metadata.sha256.clone()) {
                warnings.push(format!(
                    "Attachment '{}' duplicate in batch (sha256:{}) - skipping redundant blob.",
                    att.metadata.filename,
                    &att.metadata.sha256[..att.metadata.sha256.len().min(8)]
                ));
                continue;
            }
            if let Ok(existing_refs) = self
                .storage
                .find_attachments_by_sha256(&att.metadata.sha256)
            {
                if !existing_refs.is_empty() {
                    let (ref ex_mem_id, ref ex_filename) = existing_refs[0];
                    let short_mem: String = ex_mem_id.chars().take(8).collect();
                    warnings.push(format!(
                        "Attachment '{}' already exists in vault ('{}', memory {}) - deduplicated.",
                        att.metadata.filename, ex_filename, short_mem
                    ));
                }
            }
            if !tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&att.metadata.media_kind))
            {
                tags.push(att.metadata.media_kind.clone());
            }
            if let Some(ext) = std::path::Path::new(&att.metadata.filename)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
            {
                if !ext.is_empty() && !tags.iter().any(|t| t.eq_ignore_ascii_case(&ext)) {
                    tags.push(ext);
                }
            }
            attachment_inputs.push(att);
        }

        let user_text = content.map(str::trim).filter(|s| !s.is_empty());
        let mut content_parts: Vec<String> = Vec::new();
        if let Some(txt) = user_text {
            content_parts.push(txt.to_string());
        }

        if extract_llm {
            for att in &attachment_inputs {
                let extracted = ObserverService::extract_attachment_via_llm(
                    att,
                    user_text,
                    reranker_override,
                    &self.config,
                )?;
                content_parts.push(format!("[{}]: {}", att.metadata.filename, extracted));
            }
        }

        let final_content = if !content_parts.is_empty() {
            content_parts.join("\n\n")
        } else if !attachment_inputs.is_empty() {
            attachment_inputs
                .iter()
                .map(|a| {
                    let m = &a.metadata;
                    let mut extra = Vec::new();
                    if let Some(ref dims) = m.dimensions {
                        extra.push(dims.clone());
                    }
                    if let Some(dur) = m.duration_secs {
                        extra.push(format!("{:.1}s", dur));
                    }
                    extra.push(format!("{} bytes", m.size_bytes));
                    extra.push(format!("sha256:{}", &m.sha256[..m.sha256.len().min(12)]));
                    format!(
                        "Attached {} {} ({}, {})",
                        m.media_kind,
                        m.filename,
                        m.mime_type,
                        extra.join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        } else {
            anyhow::bail!("Memory content cannot be blank");
        };

        let resolved_type = category.unwrap_or_else(|| {
            if user_text.is_some() || extract_llm || attachment_inputs.is_empty() {
                MemoryType::infer(&final_content)
            } else {
                MemoryType::Artifact
            }
        });

        let resolved_title = title.unwrap_or_else(|| {
            if let Some(txt) = user_text {
                txt.lines()
                    .next()
                    .unwrap_or("Untitled Memory")
                    .chars()
                    .take(80)
                    .collect()
            } else if attachment_inputs.len() == 1 {
                attachment_inputs[0].metadata.filename.clone()
            } else if !attachment_inputs.is_empty() {
                format!(
                    "{} (+{} more)",
                    attachment_inputs[0].metadata.filename,
                    attachment_inputs.len() - 1
                )
            } else {
                final_content
                    .lines()
                    .next()
                    .unwrap_or("Untitled Memory")
                    .chars()
                    .take(80)
                    .collect()
            }
        });

        let conf = confidence.unwrap_or(0.9);
        anyhow::ensure!(
            conf.is_finite() && (0.0..=1.0).contains(&conf),
            "Confidence must be between 0 and 1"
        );
        let clean_provenance = provenance
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| {
                if !attachment_inputs.is_empty() {
                    if extract_llm {
                        "attachment_llm_extracted".to_string()
                    } else {
                        "attachment_metadata".to_string()
                    }
                } else {
                    "explicit_statement".to_string()
                }
            });

        let mut memory = MemoryRecord::new(
            resolved_type,
            resolved_title,
            final_content.clone(),
            tags,
            conf,
            Some(clean_provenance),
        );
        if let Some(ttl_duration) = ttl {
            memory.expired_at = Some(Utc::now() + ttl_duration);
        }
        for att in &mut attachment_inputs {
            att.metadata.memory_id = memory.id.clone();
        }
        memory.attachments = attachment_inputs
            .iter()
            .map(|a| a.metadata.clone())
            .collect();

        // Automatic entity extraction via LLM if enabled
        let mut entity_warning = None;
        if extract_entities {
            match ObserverService::extract_entities_via_llm(
                &final_content,
                reranker_override,
                &self.config,
            ) {
                Ok(extracted_ents) => {
                    memory.entities = extracted_ents;
                }
                Err(err) => {
                    entity_warning = Some(format!(
                        "Could not extract entities (LLM unreachable or error: {}). Memory saved without entities.",
                        err
                    ));
                }
            }
        }

        self.storage
            .check_embedding_identity(&self.embedding_identity)?;

        let mut stored = if !attachment_inputs.is_empty() {
            self.storage.insert_indexed_with_attachments(
                &memory,
                &attachment_inputs,
                self.embedder()?.as_ref(),
                &self.embedding_identity,
                true,
            )?
        } else {
            let mut batch = self.storage.insert_indexed_batch(
                &[memory],
                self.embedder()?.as_ref(),
                &self.embedding_identity,
                true,
            )?;
            batch.remove(0)
        };

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
                stored = self.merge_conflict_pair(&conflict, reranker_override)?;
            }
            conflicts.push(conflict);
        }

        let _ = self
            .storage
            .auto_link_memory(&stored.id, &format!("{}\n{}", stored.title, stored.content));

        let mut final_warnings = warnings;
        if let Some(ew) = entity_warning {
            final_warnings.push(ew);
        }
        let warning_output = if final_warnings.is_empty() {
            None
        } else {
            Some(final_warnings.join(" | "))
        };

        Ok((stored, conflicts, warning_output))
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
        let _ = self
            .storage
            .copy_attachments_between_memories(&older.id, &merged.id)?;
        let _ = self
            .storage
            .copy_links_between_memories(&older.id, &merged.id)?;
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
        // Day filtering must see the complete eligible pool, not a global top-250
        // prefix. Each search lane is bounded by at least this many records, so
        // no day-eligible hit can be discarded before the day predicate runs.
        let fetch_limit = if on_date.is_some() && limit > 0 {
            self.storage
                .count_memories(category, Some(MemoryStatus::Active), as_of)?
                .max(limit)
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
    #[allow(clippy::too_many_arguments)]
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
        let input = crate::services::embeddings::MultimodalInput::text_only(query);
        self.recall_multimodal_expanded(
            &input,
            category,
            as_of,
            on_date,
            limit,
            min_similarity,
            multi_hop,
        )
    }

    /// Multimodal Hybrid Recall (text + optional image/audio) with optional 1-hop graph expansion
    #[allow(clippy::too_many_arguments)]
    pub fn recall_multimodal_expanded(
        &self,
        input: &crate::services::embeddings::MultimodalInput<'_>,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        on_date: Option<NaiveDate>,
        limit: usize,
        min_similarity: Option<f32>,
        multi_hop: bool,
    ) -> Result<Vec<ScoredMemory>> {
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        // Day filtering must see the complete eligible pool, not a global top-250
        // prefix. Each search lane is bounded by at least this many records, so
        // no day-eligible hit can be discarded before the day predicate runs.
        let fetch_limit = if on_date.is_some() && limit > 0 {
            self.storage
                .count_memories(category, Some(MemoryStatus::Active), as_of)?
                .max(limit)
        } else {
            limit
        };
        let embedder = self.embedder()?;
        let mut base_results = HybridSearchEngine::search_multimodal_with_identity(
            &self.storage,
            embedder.as_ref(),
            input,
            category,
            Some(MemoryStatus::Active),
            as_of,
            fetch_limit,
            min_similarity,
            Some(&self.embedding_identity),
        )?;
        if let Some(target_day) = on_date {
            base_results.retain(|r| {
                r.memory.created_at.date_naive() == target_day
                    || r.memory.updated_at.date_naive() == target_day
            });
            base_results.truncate(limit);
        }
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
                    let memory = &rel.memory;
                    if let Some(cat) = category {
                        if memory.category != cat {
                            continue;
                        }
                    }
                    if let Some(at) = as_of {
                        if memory.created_at > at {
                            continue;
                        }
                        if let Some(expired) = memory.expired_at {
                            if expired <= at {
                                continue;
                            }
                        }
                    }
                    if let Some(day) = on_date {
                        if memory.created_at.date_naive() != day
                            && memory.updated_at.date_naive() != day
                        {
                            continue;
                        }
                    }
                    if seen_ids.insert(memory.id.clone()) {
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
        expanded.sort_by(|a, b| b.score.total_cmp(&a.score));
        expanded.truncate(limit);
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

    /// Retrieve single memory by ID or title
    pub fn get(&self, id: &str) -> Result<Option<MemoryRecord>> {
        self.storage.get_memory_by_id_or_title(id)
    }

    /// Find memories containing a specific entity name and optional value
    pub fn find_by_entity(&self, name: &str, val: Option<&str>) -> Result<Vec<MemoryRecord>> {
        self.storage.find_by_entity(name, val)
    }

    /// Access underlying SQLite storage engine
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Retrieve verified binary attachment payloads `(AttachmentMetadata, Vec<u8>)` for a memory
    pub fn get_attachments_data(
        &self,
        id_or_title: &str,
    ) -> Result<Vec<(AttachmentMetadata, Vec<u8>)>> {
        self.storage.get_memory_attachments_data(id_or_title)
    }

    /// Export all attachments of a memory to a target directory or file path on disk (verifying SHA-256 integrity)
    pub fn save_attachments_to(&self, id_or_title: &str, target: &Path) -> Result<Vec<PathBuf>> {
        self.storage.save_attachments_to(id_or_title, target)
    }

    /// Export all attachments across multiple memories to a target directory or file path on disk (verifying SHA-256 integrity)
    pub fn save_memories_attachments_to(
        &self,
        id_or_titles: &[String],
        target: &Path,
    ) -> Result<Vec<PathBuf>> {
        self.storage
            .save_memories_attachments_to(id_or_titles, target)
    }

    /// Default vault attachments cache directory (`<db_parent>/.lightmem_attachments`)
    pub fn default_attachments_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join(".lightmem_attachments")
    }

    /// Materialize and verify all attachments of a memory into `save_to` (or `default_attachments_dir()` if `None`), returning their absolute paths
    pub fn materialize_attachments(
        &self,
        id_or_title: &str,
        save_to: Option<&Path>,
    ) -> Result<Vec<PathBuf>> {
        self.materialize_memories_attachments(&[id_or_title.to_string()], save_to)
    }

    /// Materialize and verify all attachments across multiple memories into `save_to` (or `default_attachments_dir()` if `None`), returning their absolute paths
    pub fn materialize_memories_attachments(
        &self,
        id_or_titles: &[String],
        save_to: Option<&Path>,
    ) -> Result<Vec<PathBuf>> {
        let target = save_to
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.default_attachments_dir());
        let paths = self
            .storage
            .save_memories_attachments_to(id_or_titles, &target)?;
        Ok(paths
            .into_iter()
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
            .collect())
    }

    /// Forget (soft-delete or hard delete) a memory
    pub fn forget(&self, id: &str, hard_delete: bool) -> Result<bool> {
        let old_atts = if hard_delete {
            self.storage
                .get_memory(id)?
                .map(|m| m.attachments)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let ok = self.storage.forget_memory(id, hard_delete)?;
        if ok && hard_delete && !old_atts.is_empty() {
            let att_dir = self.default_attachments_dir();
            if att_dir.is_dir() {
                for att in &old_atts {
                    if !self
                        .storage
                        .is_attachment_sha_referenced(&att.sha256)
                        .unwrap_or(true)
                    {
                        if let Ok(entries) = std::fs::read_dir(&att_dir) {
                            for entry in entries.flatten() {
                                let p = entry.path();
                                if p.is_file() {
                                    if let Ok(bytes) = std::fs::read(&p) {
                                        if bytes.len() as u64 == att.size_bytes
                                            && compute_sha256_hex(&bytes) == att.sha256
                                        {
                                            let _ = std::fs::remove_file(&p);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(ok)
    }

    /// Clear (soft-retire or permanently delete) all memories in the database
    pub fn clear_all(&self, hard_delete: bool) -> Result<usize> {
        let removed = self.storage.clear_all(hard_delete)?;
        if hard_delete {
            let att_dir = self.default_attachments_dir();
            if att_dir.is_dir()
                && att_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s == ".lightmem_attachments")
                    .unwrap_or(false)
            {
                let _ = std::fs::remove_dir_all(&att_dir);
            }
        }
        Ok(removed)
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
    pub fn graph(&self, focus: Option<&str>, hops: Option<usize>) -> Result<GraphSnapshot> {
        self.storage.get_graph_snapshot(focus, hops)
    }

    /// Export memories to an OKF bundle
    pub fn export_okf(&self, target_path: Option<&Path>) -> Result<PathBuf> {
        Exporter::export_okf(&self.storage, target_path)
    }

    pub fn export_json(&self, target_path: &Path) -> Result<PathBuf> {
        Exporter::export_json(&self.storage, target_path)
    }

    /// Export the entire database, all videos, photos, documents, and vectors into a compressed ZIP transfer bundle
    pub fn export_bundle(&self, target_path: Option<&Path>) -> Result<PathBuf> {
        crate::services::BundleService::export_bundle(&self.storage, target_path)
    }

    /// Read and inspect a transfer bundle's manifest without unpacking
    pub fn inspect_bundle(&self, bundle_path: &Path) -> Result<crate::services::BundleManifest> {
        crate::services::BundleService::inspect_bundle(bundle_path)
    }

    /// Import a transfer bundle, restoring or merging the DB and materializing all video/photo files
    pub fn import_bundle(
        &self,
        bundle_path: &Path,
        replace: bool,
        merge: bool,
    ) -> Result<crate::services::BundleImportResult> {
        crate::services::BundleService::import_bundle(&self.storage, bundle_path, replace, merge)
    }

    /// Import memories from an external file (.json, .jsonl, or .md/.okf) or an OKF bundle directory
    pub fn import_file(&self, file_path: &Path) -> Result<usize> {
        self.import_file_with_options(file_path, false, false)
    }

    /// Parse every file before changing the database; one failed record rolls back the entire import.
    pub fn import_file_with_enrichment(&self, file_path: &Path, enrich: bool) -> Result<usize> {
        self.import_file_with_options(file_path, enrich, false)
    }

    pub fn import_file_with_options(
        &self,
        file_path: &Path,
        enrich: bool,
        replace: bool,
    ) -> Result<usize> {
        if !file_path.is_dir() {
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("lmbundle") {
                let res = self.import_bundle(file_path, replace, false)?;
                return Ok(res.memories_imported);
            }
        }
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
        let mut explicit_ids = Vec::new();
        let mut attachments_by_mem = Vec::new();
        let mut imported_links = Vec::new();
        let mut restore = false;
        for file in files {
            let raw = std::fs::read_to_string(&file)
                .with_context(|| format!("Failed to read {}", file.display()))?;
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_markdown_or_okf = ext.eq_ignore_ascii_case("md")
                || ext.eq_ignore_ascii_case("okf")
                || ext.eq_ignore_ascii_case("markdown");
            let json = if is_markdown_or_okf {
                false
            } else {
                ext.eq_ignore_ascii_case("json")
                    || ext.eq_ignore_ascii_case("jsonl")
                    || raw.trim_start().starts_with(['{', '['])
            };
            let candidates = if json {
                JsonMemoryImporter.parse_with_enrichment(&raw, enrich)?
            } else {
                OkfMemoryImporter.parse(&raw)?
            };
            imported_links.extend(extract_imported_links(&raw, json));
            restore |= candidates.iter().any(|c| c.status.is_some());
            for candidate in candidates {
                anyhow::ensure!(
                    candidate.confidence.is_finite() && (0.0..=1.0).contains(&candidate.confidence),
                    "Invalid import confidence"
                );
                if let Some(ref id) = candidate.id {
                    explicit_ids.push(id.clone());
                }
                let (record, att_inputs) = candidate.into_record_and_attachments()?;
                if !att_inputs.is_empty() {
                    attachments_by_mem.push((record.id.clone(), att_inputs));
                }
                records.push(record);
            }
        }
        if records.is_empty() {
            return Ok(0);
        }
        if !replace && !explicit_ids.is_empty() {
            let conflicts = self.storage.find_existing_ids(&explicit_ids)?;
            if !conflicts.is_empty() {
                anyhow::bail!(
                    "Import rejected: {} memories have conflicting IDs (e.g. '{}'). Pass --replace to explicitly overwrite existing records.",
                    conflicts.len(),
                    conflicts[0]
                );
            }
        }
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let stored_records = self.storage.insert_indexed_batch_with_attachments(
            &records,
            &attachments_by_mem,
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            !restore,
        )?;
        if !imported_links.is_empty() {
            self.storage.restore_links(&imported_links)?;
        }
        for rec in &stored_records {
            if rec.status == MemoryStatus::Active {
                let _ = self
                    .storage
                    .auto_link_memory(&rec.id, &format!("{}\n{}", rec.title, rec.content));
            }
        }
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

        if lower == "ollama" || lower == "openai" || lower == "llm" {
            let cfg_ollama = self
                .config
                .reranker
                .strip_prefix("ollama:")
                .or_else(|| self.config.reranker.strip_prefix("OLLAMA:"))
                .or_else(|| self.config.reranker.strip_prefix("openai:"))
                .or_else(|| self.config.reranker.strip_prefix("OPENAI:"))
                .or_else(|| self.config.reranker.strip_prefix("llm:"))
                .or_else(|| self.config.reranker.strip_prefix("LLM:"))
                .map(|s| s.to_string());
            return Ok(Box::new(OllamaReranker::with_api_key(
                self.config.effective_ollama_url(),
                cfg_ollama,
                self.config.effective_ollama_api_key(),
            )));
        }

        if let Some(ollama_model) = active_reranker
            .strip_prefix("ollama:")
            .or_else(|| active_reranker.strip_prefix("OLLAMA:"))
            .or_else(|| active_reranker.strip_prefix("openai:"))
            .or_else(|| active_reranker.strip_prefix("OPENAI:"))
            .or_else(|| active_reranker.strip_prefix("llm:"))
            .or_else(|| active_reranker.strip_prefix("LLM:"))
        {
            return Ok(Box::new(OllamaReranker::with_api_key(
                self.config.effective_ollama_url(),
                Some(ollama_model.to_string()),
                self.config.effective_ollama_api_key(),
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
                "Unknown reranker: '{}'. Choose 'minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama' ('ollama:<model>'), 'openai' ('openai:<model>'), or 'top1'.",
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

        // Exact entity slot shortcut (when caller did not explicitly request a specific reranker
        // and the query is not a multi-record synthesis/summary prompt).
        if reranker_override.is_none() && !crate::services::reranker::is_synthesis_query(question) {
            let tokenize = |s: &str| -> Vec<String> {
                s.to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(ToString::to_string)
                    .collect()
            };
            let q_words = tokenize(question);
            for cand in &candidates {
                for ent in &cand.memory.entities {
                    let name_words = tokenize(&ent.name);
                    let val_words = tokenize(&ent.val);
                    if name_words.is_empty() || val_words.is_empty() {
                        continue;
                    }
                    // Skip tautological self-entities (e.g. name="lightmem", val="LightMem")
                    if name_words == val_words {
                        continue;
                    }
                    // Skip if the entity value is already explicitly present in the question
                    let val_in_question = q_words
                        .windows(val_words.len())
                        .any(|w| w == val_words.as_slice());
                    if val_in_question {
                        continue;
                    }
                    // Require whole-token/phrase match of the entity slot name in the question
                    let name_in_question = q_words
                        .windows(name_words.len())
                        .any(|w| w == name_words.as_slice());
                    if name_in_question {
                        return Ok(AnswerResult {
                            answer: ent.val.clone(),
                            selected_memory: Some(cand.memory.clone()),
                            confidence: cand.memory.confidence.max(0.95),
                            reranker_used: "entity_index".to_string(),
                        });
                    }
                }
            }
        }

        let reranker = self.build_reranker(reranker_override)?;
        reranker.answer(question, &candidates)
    }

    /// Extract candidate observations/facts from conversation or text.
    pub fn observe_and_extract(
        &self,
        raw_text: &str,
        category_override: Option<MemoryType>,
        extra_tags: Option<&[String]>,
        confidence_threshold: Option<f32>,
        reranker_spec: Option<&str>,
    ) -> Result<Vec<ObservedCandidate>> {
        ObserverService::extract(
            raw_text,
            category_override,
            extra_tags,
            confidence_threshold,
            reranker_spec,
            Some(&self.config),
        )
    }

    /// Save an individual observed candidate memory into SQLite and index it.
    pub fn save_observed_candidate(&self, candidate: &ObservedCandidate) -> Result<MemoryRecord> {
        let memory = candidate.to_memory_record();
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let mut stored_batch = self.storage.insert_indexed_batch(
            &[memory],
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            true,
        )?;
        let stored = stored_batch.remove(0);
        let _ = self
            .storage
            .auto_link_memory(&stored.id, &format!("{}\n{}", stored.title, stored.content));
        Ok(stored)
    }

    /// Batch save multiple observed candidate memories into SQLite and index them.
    pub fn save_observed_candidates(
        &self,
        candidates: &[ObservedCandidate],
    ) -> Result<Vec<MemoryRecord>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let records: Vec<MemoryRecord> = candidates.iter().map(|c| c.to_memory_record()).collect();
        let stored_batch = self.storage.insert_indexed_batch(
            &records,
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            true,
        )?;
        for rec in &stored_batch {
            let _ = self
                .storage
                .auto_link_memory(&rec.id, &format!("{}\n{}", rec.title, rec.content));
        }
        Ok(stored_batch)
    }
}
