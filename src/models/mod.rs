pub mod identity;
pub use identity::{
    inspect_vault_db, DefaultVaultStatus, IdentityEntry, IdentityRegistry, MemoryContext,
    MemoryScope, ResolvedConnection, VaultConnectionStatus, VaultStatusOverview,
};
pub mod config;
pub mod link;
pub mod memory;
pub mod stats;

pub use config::LightMemConfig;
pub use link::{extract_wikilinks, GraphEdge, GraphNode, GraphSnapshot, MemoryLink, RelatedMemory};
pub use memory::{
    compute_sha256_hex, decode_base64, detect_memory_conflict,
    detect_memory_conflict_with_similarity, detect_mime_and_kind, encode_base64,
    extract_meaningful_tokens, parse_audio_duration_secs, parse_image_dimensions,
    parse_mp4_dimensions_and_duration, parse_ttl_duration, AttachmentInput, AttachmentMetadata,
    EntityRecord, MemoryConflict, MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories,
    ScoredMemory, DEFAULT_CONFLICT_SIMILARITY,
};
pub use stats::StorageStats;

#[derive(Debug, thiserror::Error, Clone, serde::Serialize, serde::Deserialize)]
#[error("Model request to {endpoint} (model '{model}') failed: {details}")]
pub struct ModelError {
    pub endpoint: String,
    pub model: String,
    pub details: String,
    pub status_code: Option<u16>,
}
