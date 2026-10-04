use crate::models::{
    LightMemConfig, MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories, ScoredMemory,
    StorageStats,
};
use crate::repositories::Storage;
use crate::services::{
    AnswerResult, EmbeddingProvider, Exporter, HashEmbeddingProvider, HybridSearchEngine,
    JsonMemoryImporter, MemoryImporter, NeedleReranker, OkfMemoryImporter, OllamaEmbeddingProvider,
    OnnxEmbeddingProvider, Reranker, Top1Reranker,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
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
        let memory = MemoryRecord::new(
            resolved_type,
            resolved_title,
            clean_content,
            tags,
            conf,
            Some("explicit_statement".to_string()),
        );

        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        let mut stored = self.storage.insert_indexed_batch(
            &[memory],
            self.embedder()?.as_ref(),
            &self.embedding_identity,
            true,
        )?;
        Ok(stored.remove(0))
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
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        HybridSearchEngine::search_with_identity(
            &self.storage,
            self.embedder()?.as_ref(),
            query,
            category,
            Some(MemoryStatus::Active),
            as_of,
            limit,
            min_similarity,
            Some(&self.embedding_identity),
        )
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
        self.storage
            .list_memories_paginated(category, status, as_of, limit, offset)
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
        anyhow::ensure!(
            page > 0 && per_page > 0,
            "Page and page size must be positive"
        );
        let per_page_clean = per_page;
        let offset = (page - 1)
            .checked_mul(per_page_clean)
            .context("Pagination offset is too large")?;
        self.storage
            .list_memories_paginated(category, status, as_of, per_page_clean, offset)
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

    /// Scan active memories, smart-merge duplicate content (union tags, max confidence, best title, earliest created_at), and delete redundant rows
    pub fn deduplicate(&self) -> Result<usize> {
        self.storage
            .check_embedding_identity(&self.embedding_identity)?;
        self.storage
            .deduplicate_indexed(self.embedder()?.as_ref(), &self.embedding_identity)
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

    /// Answer a natural language question using retrieved memory candidates
    /// and either Top-1 direct selection (0ms) or Native Needle 3 precision disambiguation & slot extraction.
    pub fn answer(
        &self,
        question: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        use_needle: bool,
    ) -> Result<AnswerResult> {
        let override_reranker = if use_needle { Some("needle") } else { None };
        self.answer_with_reranker(question, category, as_of, limit, override_reranker)
    }

    /// Answer with an explicit per-query reranker override ("top1" or "needle")
    pub fn answer_with_reranker(
        &self,
        question: &str,
        category: Option<MemoryType>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        reranker_override: Option<&str>,
    ) -> Result<AnswerResult> {
        let candidates = self.recall(question, category, as_of, limit, None)?;

        let active_reranker = reranker_override.unwrap_or(&self.config.reranker);
        anyhow::ensure!(
            ["top1", "needle", "precision"].contains(&active_reranker.to_lowercase().as_str()),
            "Unknown reranker: {}",
            active_reranker
        );
        let needle_enabled = active_reranker.eq_ignore_ascii_case("needle")
            || active_reranker.eq_ignore_ascii_case("precision");

        if needle_enabled {
            let reranker = NeedleReranker;
            reranker.answer(question, &candidates)
        } else {
            let reranker = Top1Reranker;
            reranker.answer(question, &candidates)
        }
    }
}
