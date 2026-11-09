pub mod controllers;
pub mod models;
pub mod repositories;
pub mod services;
pub mod views;

// Module aliases for seamless internal & external imports
pub use models::config;
pub use repositories::sqlite_repo as storage;
pub use services::{bundle, connect, embeddings, exporter, importer, observer, reranker, search};

// Primary SDK Re-exports
pub use controllers::{LightMem, MemoryController};
pub use models::identity::{
    inspect_vault_db, DefaultVaultStatus, IdentityEntry, IdentityRegistry, MemoryContext,
    MemoryScope, ResolvedConnection, VaultConnectionStatus, VaultStatusOverview,
};
pub use models::{
    cas_blob_filename, compute_file_sha256_hex, compute_sha256_hex, decode_base64,
    detect_memory_conflict, detect_memory_conflict_with_similarity, detect_mime_and_kind,
    encode_base64, extract_meaningful_tokens, parse_ttl_duration, AttachmentInput,
    AttachmentMetadata, EntityRecord, LightMemConfig, MemoryConflict, MemoryRecord, MemoryStatus,
    MemoryType, PaginatedMemories, ScoredMemory, Sha256Hasher, StorageStats,
};
pub use repositories::{SqliteRepository, Storage};
#[cfg(feature = "mcp")]
pub use services::McpServer;
pub use services::{
    serve_docs, AnswerResult, BundleAttachmentEntry, BundleImportResult, BundleManifest,
    BundleService, BundleStats, ConnectResult, ConnectService, DocsOptions, DriveAuthStatus,
    DriveFolders, DriveLogoutReport, EmbeddingProvider, Exporter, GoogleDriveClient,
    HashEmbeddingProvider, HybridSearchEngine, ImportCandidate, JsonMemoryImporter, LlmReranker,
    MemoryImporter, MultimodalInput, ObservedCandidate, ObserverService, OkfMemoryImporter,
    OllamaEmbeddingProvider, OllamaReranker, OnnxEmbeddingProvider, OnnxQaReranker, PlatformInfo,
    PrecisionReranker, Reranker, SyncManifest, SyncOptions, SyncProgress, SyncPullReport,
    SyncReport, SyncService, SyncStatusReport, Top1Reranker,
};

pub use views::CliView;
