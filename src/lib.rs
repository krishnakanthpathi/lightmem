pub mod controllers;
pub mod models;
pub mod repositories;
pub mod services;
pub mod views;

// Module aliases for seamless internal & external imports
pub use models::config;
pub use repositories::sqlite_repo as storage;
pub use services::{connect, embeddings, exporter, importer, reranker, search};

// Primary SDK Re-exports
pub use controllers::{LightMem, MemoryController};
pub use models::{
    detect_memory_conflict, detect_memory_conflict_with_similarity, extract_meaningful_tokens,
    parse_ttl_duration, LightMemConfig, MemoryConflict, MemoryRecord, MemoryStatus, MemoryType,
    PaginatedMemories, ScoredMemory, StorageStats,
};
pub use repositories::{SqliteRepository, Storage};
pub use services::{
    AnswerResult, ConnectResult, ConnectService, EmbeddingProvider, Exporter,
    HashEmbeddingProvider, HybridSearchEngine, ImportCandidate, JsonMemoryImporter, MemoryImporter,
    OkfMemoryImporter, OllamaEmbeddingProvider, OllamaReranker, LlmReranker, OnnxEmbeddingProvider,
    OnnxQaReranker, PlatformInfo, PrecisionReranker, Reranker, Top1Reranker,
};
pub use views::CliView;
