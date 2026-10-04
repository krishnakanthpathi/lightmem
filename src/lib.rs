pub mod controllers;
pub mod models;
pub mod repositories;
pub mod services;
pub mod views;

// Module aliases for seamless internal & external imports
pub use models::config;
pub use repositories::sqlite_repo as storage;
pub use services::{embeddings, exporter, importer, reranker, search};

// Primary SDK Re-exports
pub use controllers::{LightMem, MemoryController};
pub use models::{
    detect_memory_conflict, parse_ttl_duration, LightMemConfig, MemoryConflict, MemoryRecord,
    MemoryStatus, MemoryType, PaginatedMemories, ScoredMemory, StorageStats,
};
pub use repositories::{SqliteRepository, Storage};
pub use services::{
    AnswerResult, EmbeddingProvider, Exporter, HashEmbeddingProvider, HybridSearchEngine,
    ImportCandidate, JsonMemoryImporter, MemoryImporter, NeedleReranker, OkfMemoryImporter,
    OllamaEmbeddingProvider, OllamaReranker, OnnxEmbeddingProvider, OnnxQaReranker,
    PrecisionReranker, Reranker, Top1Reranker,
};
pub use views::CliView;
