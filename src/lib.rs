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
    compute_sha256_hex, decode_base64, detect_memory_conflict,
    detect_memory_conflict_with_similarity, detect_mime_and_kind, encode_base64,
    extract_meaningful_tokens, parse_ttl_duration, AttachmentInput, AttachmentMetadata,
    EntityFilter, EntityRecord, EntitySummary, LightMemConfig, ListSort, MemoryConflict,
    MemoryRecord, MemoryStatus, MemoryType, ModelError, PaginatedMemories, ScoredMemory,
    StorageStats,
};
pub use repositories::{SqliteRepository, Storage};
#[cfg(feature = "mcp")]
pub use services::McpServer;
pub use services::{
    serve_api, serve_docs, AnswerRequest, AnswerResult, AttachmentPayload, AutolinkRequest,
    BundleAttachmentEntry, BundleImportResult, BundleManifest, BundleService, BundleStats,
    ClearPayload, CommandHistoryEntry, ConflictsRequest, ConnectResult, ConnectService,
    DocsOptions, EmbeddingProvider, ExportFilter, Exporter, ForgetPayload, HashEmbeddingProvider,
    HistoryService, HybridSearchEngine, ImportCandidate, JsonMemoryImporter, LinkRequest,
    LinkResult, LlmReranker, MemoryImporter, MultimodalInput, ObserveRequest, ObservedCandidate,
    ObserverService, OkfMemoryImporter, OllamaEmbeddingProvider, OllamaReranker,
    OnnxEmbeddingProvider, OnnxQaReranker, PlatformInfo, PrecisionReranker, RecallRequest,
    RememberRequest, RemoteClient, Reranker, ServerOptions, Top1Reranker, UnlinkRequest,
    UpdateRequest,
};

pub use views::{CliErrorCard, CliView};
