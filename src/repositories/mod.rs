pub mod sqlite_repo;

pub use crate::models::StorageStats;
pub use sqlite_repo::Storage;
pub type SqliteRepository = Storage;
