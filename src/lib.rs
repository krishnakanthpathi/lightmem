pub mod controllers;
pub mod models;
pub mod repositories;
pub mod services;
pub mod views;

// Module aliases for seamless internal & external imports
pub use models::config;
pub use repositories::sqlite_repo as storage;
pub use services::{connect, embeddings, exporter, importer, observer, reranker, search};

// Primary SDK Re-exports
pub use controllers::{LightMem, MemoryController};
pub use models::{
    compute_sha256_hex, decode_base64, detect_memory_conflict,
    detect_memory_conflict_with_similarity, detect_mime_and_kind, encode_base64,
    extract_meaningful_tokens, parse_ttl_duration, AttachmentInput, AttachmentMetadata,
    LightMemConfig, MemoryConflict, MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories,
    ScoredMemory, StorageStats,
};
pub use repositories::{SqliteRepository, Storage};
pub use services::{
    AnswerResult, ConnectResult, ConnectService, EmbeddingProvider, Exporter,
    HashEmbeddingProvider, HybridSearchEngine, ImportCandidate, JsonMemoryImporter, LlmReranker,
    MemoryImporter, ObservedCandidate, ObserverService, OkfMemoryImporter, OllamaEmbeddingProvider,
    OllamaReranker, OnnxEmbeddingProvider, OnnxQaReranker, PlatformInfo, PrecisionReranker,
    Reranker, Top1Reranker,
};
#[cfg(feature = "mcp")]
pub use services::McpServer;

pub use views::CliView;
