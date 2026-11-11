pub mod bundle;
pub mod connect;
pub mod docs_server;
pub mod embeddings;
pub mod exporter;
pub mod gdrive;
pub mod graph_view;
pub mod importer;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod observer;
pub mod reranker;
pub mod search;
pub mod sync;

pub use bundle::{
    BundleAttachmentEntry, BundleImportResult, BundleManifest, BundleService, BundleStats,
};
pub use connect::{ConnectResult, ConnectService, PlatformInfo};
pub use docs_server::{serve_docs, DocsOptions};
pub use gdrive::{DriveAuthStatus, DriveFolders, DriveLogoutReport, GoogleDriveClient};
#[cfg(feature = "mcp")]
pub use mcp::McpServer;
pub use sync::{
    InlinePullOutcome, SyncManifest, SyncOptions, SyncProgress, SyncPullReport, SyncReport,
    SyncService, SyncState, SyncStatusReport,
};

pub use embeddings::{
    cosine_similarity, EmbeddingProvider, HashEmbeddingProvider, MultimodalInput,
    OllamaEmbeddingProvider, OnnxEmbeddingProvider,
};
pub use exporter::Exporter;
pub use graph_view::render_terminal;
pub use importer::{
    extract_imported_links, ImportCandidate, JsonMemoryImporter, MemoryImporter, OkfMemoryImporter,
};
pub use observer::{ObservedCandidate, ObserverService};
pub use reranker::{
    AnswerResult, LlmReranker, OllamaReranker, OnnxQaReranker, PrecisionReranker, Reranker,
    Top1Reranker,
};
pub use search::HybridSearchEngine;
