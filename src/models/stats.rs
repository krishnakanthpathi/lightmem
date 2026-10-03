use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_memories: usize,
    pub active_memories: usize,
    pub expired_memories: usize,
    pub total_vectors: usize,
    pub by_category: Vec<(String, usize)>,
}
