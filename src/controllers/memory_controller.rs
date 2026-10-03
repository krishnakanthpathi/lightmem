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
}

pub type MemoryController = LightMem;

impl LightMem {
    /// Open the active database based on configuration (local project or global)
    pub fn open_default(force_global: bool) -> Result<Self> {
        let config = LightMemConfig::load();
        let db_path = LightMemConfig::resolve_db_path(force_global);
        Self::open_at(&db_path, config)
    }

    /// Open database at a specific path (embedding model is loaded lazily on first use)
    pub fn open_at(db_path: &Path, config: LightMemConfig) -> Result<Self> {
        let storage = Storage::open(db_path)?;

        Ok(Self {
            storage: Arc::new(storage),
            embedder: OnceLock::new(),
            config,
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
                    match OnnxEmbeddingProvider::new_custom_dir(path) {
                        Ok(p) => Arc::new(p),
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to load custom ONNX model from {:?}: {}. Falling back to default bge-small",
                                path, e
                            );
                            Arc::new(OnnxEmbeddingProvider::new(Some("bge-small"))?)
                        }
                    }
                } else {
                    Arc::new(OnnxEmbeddingProvider::new(Some(model_str))?)
                }
            }
            "ollama" => Arc::new(OllamaEmbeddingProvider::new(
                self.config.ollama_url.clone(),
                self.config.embedding_model.clone(),
            )),
            _ => Arc::new(HashEmbeddingProvider),
        };

        let _ = self.embedder.set(created);
        Ok(self.embedder.get().unwrap())
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
        let memory = MemoryRecord::new(
            resolved_type,
            resolved_title,
            clean_content,
            tags,
            conf,
            Some("explicit_statement".to_string()),
        );

        let card_text = memory.to_card_text();
        let vector = self.embedder()?.embed(&card_text).ok();

        self.storage.insert_memory(&memory, vector.as_deref())?;
        Ok(memory)
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
        HybridSearchEngine::search(
            &self.storage,
            self.embedder()?.as_ref(),
            query,
            category,
            Some(MemoryStatus::Active),
            as_of,
            limit,
            min_similarity,
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
        let per_page_clean = per_page.max(1);
        let offset = page.saturating_sub(1) * per_page_clean;
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

    /// Export memories to an OKF bundle
    pub fn export_okf(&self, target_path: Option<&Path>) -> Result<PathBuf> {
        Exporter::export_okf(&self.storage, target_path)
    }

    /// Import memories from an external file (.json, .jsonl, or .md/.okf) or an OKF bundle directory
    pub fn import_file(&self, file_path: &Path) -> Result<usize> {
        if file_path.is_dir() {
            let root = if file_path.join("memories").is_dir() {
                file_path.join("memories")
            } else {
                file_path.to_path_buf()
            };
            let mut files = Vec::new();
            Self::collect_importable_files(&root, &mut files)?;
            files.sort();

            let mut total = 0;
            for f in files {
                total += self.import_single_file(&f)?;
            }
            return Ok(total);
        }

        self.import_single_file(file_path)
    }

    fn collect_importable_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory {:?}", dir))?
        {
            let entry = entry?;
            let path = entry.path();
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

    fn import_single_file(&self, file_path: &Path) -> Result<usize> {
        let raw = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read import file {:?}", file_path))?;

        let ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let is_json = ext.eq_ignore_ascii_case("json")
            || ext.eq_ignore_ascii_case("jsonl")
            || raw.trim_start().starts_with('{')
            || raw.trim_start().starts_with('[');

        let candidates = if is_json {
            JsonMemoryImporter.parse_flexible(&raw)?
        } else {
            OkfMemoryImporter.parse(&raw)?
        };

        let count = candidates.len();
        let embedder = self.embedder()?;
        for candidate in candidates {
            let memory = candidate.to_memory_record();
            let card_text = memory.to_card_text();
            let vector = embedder.embed(&card_text).ok();
            self.storage.insert_memory(&memory, vector.as_deref())?;
        }

        Ok(count)
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
        let needle_enabled = active_reranker.eq_ignore_ascii_case("needle")
            || active_reranker.eq_ignore_ascii_case("precision");

        if needle_enabled {
            let reranker = NeedleReranker::default();
            reranker.answer(question, &candidates)
        } else {
            let reranker = Top1Reranker;
            reranker.answer(question, &candidates)
        }
    }
}
