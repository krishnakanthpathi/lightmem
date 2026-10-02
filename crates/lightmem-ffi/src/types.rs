use lightmem_core::{
    MemoryRecord as CoreRecord, MemoryStatus as CoreStatus, MemoryType as CoreType,
    ScoredMemory as CoreScoredMemory, StorageStats as CoreStorageStats,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LightMemFfiError {
    #[error("Storage error: {msg}")]
    StorageError { msg: String },
    #[error("Invalid parameter: {msg}")]
    InvalidInput { msg: String },
    #[error("Serialization error: {msg}")]
    SerializationError { msg: String },
}

impl From<anyhow::Error> for LightMemFfiError {
    fn from(err: anyhow::Error) -> Self {
        Self::StorageError {
            msg: err.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct FfiMemoryRecord {
    pub id: String,
    pub category: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub status: String,
    pub provenance: String,
    pub created_at: String,
    pub updated_at: String,
    pub expired_at: Option<String>,
}

impl From<&CoreRecord> for FfiMemoryRecord {
    fn from(r: &CoreRecord) -> Self {
        Self {
            id: r.id.clone(),
            category: r.category.as_str().to_string(),
            title: r.title.clone(),
            content: r.content.clone(),
            tags: r.tags.clone(),
            confidence: r.confidence,
            status: r.status.as_str().to_string(),
            provenance: r.provenance.clone(),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            expired_at: r.expired_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct FfiScoredMemory {
    pub memory: FfiMemoryRecord,
    pub score: f32,
}

impl From<&CoreScoredMemory> for FfiScoredMemory {
    fn from(s: &CoreScoredMemory) -> Self {
        Self {
            memory: FfiMemoryRecord::from(&s.memory),
            score: s.score,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct FfiAnswerResult {
    pub answer: String,
    pub selected_memory: Option<FfiMemoryRecord>,
    pub confidence: f32,
    pub reranker_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct FfiCategoryCount {
    pub category: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct FfiStorageStats {
    pub total_memories: u64,
    pub active_memories: u64,
    pub expired_memories: u64,
    pub total_vectors: u64,
    pub by_category: Vec<FfiCategoryCount>,
}

impl From<&CoreStorageStats> for FfiStorageStats {
    fn from(s: &CoreStorageStats) -> Self {
        Self {
            total_memories: s.total_memories as u64,
            active_memories: s.active_memories as u64,
            expired_memories: s.expired_memories as u64,
            total_vectors: s.total_vectors as u64,
            by_category: s
                .by_category
                .iter()
                .map(|(cat, count)| FfiCategoryCount {
                    category: cat.clone(),
                    count: *count as u64,
                })
                .collect(),
        }
    }
}

pub fn parse_memory_type(cat: &str) -> Result<CoreType, LightMemFfiError> {
    cat.parse::<CoreType>()
        .map_err(|e| LightMemFfiError::InvalidInput { msg: e })
}

pub fn parse_memory_status(stat: &str) -> Result<CoreStatus, LightMemFfiError> {
    match stat.to_lowercase().as_str() {
        "active" => Ok(CoreStatus::Active),
        "expired" => Ok(CoreStatus::Expired),
        other => Err(LightMemFfiError::InvalidInput {
            msg: format!("Invalid memory status '{}'. Expected active or expired", other),
        }),
    }
}
