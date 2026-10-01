use crate::models::{MemoryRecord, MemoryStatus, MemoryType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_memories: usize,
    pub active_memories: usize,
    pub expired_memories: usize,
    pub total_vectors: usize,
    pub by_category: Vec<(String, usize)>,
}

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Storage {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory for {:?}", path_buf))?;
        }

        let conn = Connection::open(&path_buf)
            .with_context(|| format!("Failed to open SQLite database at {:?}", path_buf))?;

        // Set busy timeout for WAL concurrency (10 seconds)
        conn.busy_timeout(std::time::Duration::from_secs(10))?;

        // Enable WAL mode for high concurrency & speed
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path_buf,
        };

        storage.migrate()?;
        Ok(storage)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Check if schema is already migrated to avoid unnecessary DDL locks under concurrency
        let already_migrated: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if already_migrated {
            return Ok(());
        }

        conn.execute_batch(
            r#"
            -- Core memories table
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT NOT NULL,
                confidence REAL NOT NULL,
                status TEXT NOT NULL,
                provenance TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expired_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
            CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
            CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);

            -- FTS5 full-text index
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tags,
                tokenize='porter unicode61'
            );

            -- FTS synchronization triggers
            CREATE TRIGGER IF NOT EXISTS trg_memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(id, title, content, tags) 
                VALUES (new.id, new.title, new.content, new.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS trg_memories_ad AFTER DELETE ON memories BEGIN
                DELETE FROM memories_fts WHERE id = old.id;
            END;

            CREATE TRIGGER IF NOT EXISTS trg_memories_au AFTER UPDATE ON memories BEGIN
                DELETE FROM memories_fts WHERE id = old.id;
                INSERT INTO memories_fts(id, title, content, tags) 
                VALUES (new.id, new.title, new.content, new.tags);
            END;

            -- Vector embeddings table
            CREATE TABLE IF NOT EXISTS memory_vectors (
                id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                dims INTEGER NOT NULL,
                FOREIGN KEY(id) REFERENCES memories(id) ON DELETE CASCADE
            );
            "#,
        )?;

        Ok(())
    }

    pub fn insert_memory(&self, memory: &MemoryRecord, vector: Option<&[f32]>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        let tags_str = memory.tags.join(",");
        let expired_str = memory.expired_at.map(|dt| dt.to_rfc3339());

        tx.execute(
            r#"
            INSERT INTO memories (id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                memory.id,
                memory.category.as_str(),
                memory.title,
                memory.content,
                tags_str,
                memory.confidence,
                memory.status.as_str(),
                memory.provenance,
                memory.created_at.to_rfc3339(),
                memory.updated_at.to_rfc3339(),
                expired_str,
            ],
        )?;

        if let Some(v) = vector {
            let blob = serialize_f32_slice(v);
            tx.execute(
                "INSERT INTO memory_vectors (id, embedding, dims) VALUES (?1, ?2, ?3)",
                params![memory.id, blob, v.len() as i64],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_memory(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at
            FROM memories WHERE id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_memory(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn forget_memory(&self, id: &str, hard_delete: bool) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        if hard_delete {
            let rows_affected = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        } else {
            let now_str = Utc::now().to_rfc3339();
            let rows_affected = conn.execute(
                "UPDATE memories SET status = 'expired', expired_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now_str, id],
            )?;
            Ok(rows_affected > 0)
        }
    }

    pub fn list_memories(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut query = String::from(
            "SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE 1=1"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(cat) = category {
            params_vec.push(Box::new(cat.as_str().to_string()));
            query.push_str(&format!(" AND category = ?{}", params_vec.len()));
        }

        if let Some(as_of_dt) = as_of {
            let as_of_str = as_of_dt.to_rfc3339();
            params_vec.push(Box::new(as_of_str.clone()));
            query.push_str(&format!(" AND created_at <= ?{}", params_vec.len()));
            params_vec.push(Box::new(as_of_str));
            query.push_str(&format!(" AND (expired_at IS NULL OR expired_at > ?{})", params_vec.len()));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            query.push_str(&format!(" AND status = ?{}", params_vec.len()));
        }

        query.push_str(" ORDER BY created_at DESC");

        if limit > 0 {
            params_vec.push(Box::new(limit as i64));
            query.push_str(&format!(" LIMIT ?{}", params_vec.len()));
        }

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| row_to_memory(row))?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn search_bm25(
        &self,
        query: &str,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let clean_query = sanitize_fts5_query(query);
        if clean_query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            r#"
            SELECT m.id, bm25(memories_fts) as rank
            FROM memories_fts f
            JOIN memories m ON m.id = f.id
            WHERE memories_fts MATCH ?1
            "#
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(clean_query)];

        if let Some(cat) = category {
            params_vec.push(Box::new(cat.as_str().to_string()));
            sql.push_str(&format!(" AND m.category = ?{}", params_vec.len()));
        }

        if let Some(as_of_dt) = as_of {
            let as_of_str = as_of_dt.to_rfc3339();
            params_vec.push(Box::new(as_of_str.clone()));
            sql.push_str(&format!(" AND m.created_at <= ?{}", params_vec.len()));
            params_vec.push(Box::new(as_of_str));
            sql.push_str(&format!(" AND (m.expired_at IS NULL OR m.expired_at > ?{})", params_vec.len()));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            sql.push_str(&format!(" AND m.status = ?{}", params_vec.len()));
        }

        sql.push_str(" ORDER BY rank ASC");

        if limit > 0 {
            params_vec.push(Box::new(limit as i64));
            sql.push_str(&format!(" LIMIT ?{}", params_vec.len()));
        }

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let id: String = row.get(0)?;
            let rank: f64 = row.get(1)?;
            Ok((id, rank as f32))
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn get_candidate_vectors(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<(String, Vec<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            r#"
            SELECT v.id, v.embedding
            FROM memory_vectors v
            JOIN memories m ON m.id = v.id
            WHERE 1=1
            "#
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(cat) = category {
            params_vec.push(Box::new(cat.as_str().to_string()));
            sql.push_str(&format!(" AND m.category = ?{}", params_vec.len()));
        }

        if let Some(as_of_dt) = as_of {
            let as_of_str = as_of_dt.to_rfc3339();
            params_vec.push(Box::new(as_of_str.clone()));
            sql.push_str(&format!(" AND m.created_at <= ?{}", params_vec.len()));
            params_vec.push(Box::new(as_of_str));
            sql.push_str(&format!(" AND (m.expired_at IS NULL OR m.expired_at > ?{})", params_vec.len()));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            sql.push_str(&format!(" AND m.status = ?{}", params_vec.len()));
        }

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let vec = deserialize_f32_slice(&blob);
            Ok((id, vec))
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn stats(&self) -> Result<StorageStats> {
        let conn = self.conn.lock().unwrap();

        let total_memories: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        let active_memories: i64 = conn.query_row("SELECT COUNT(*) FROM memories WHERE status = 'active'", [], |r| r.get(0))?;
        let expired_memories: i64 = conn.query_row("SELECT COUNT(*) FROM memories WHERE status = 'expired'", [], |r| r.get(0))?;
        let total_vectors: i64 = conn.query_row("SELECT COUNT(*) FROM memory_vectors", [], |r| r.get(0))?;

        let mut stmt = conn.prepare("SELECT category, COUNT(*) FROM memories GROUP BY category ORDER BY COUNT(*) DESC")?;
        let rows = stmt.query_map([], |row| {
            let cat: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((cat, count as usize))
        })?;

        let mut by_category = Vec::new();
        for r in rows {
            by_category.push(r?);
        }

        Ok(StorageStats {
            total_memories: total_memories as usize,
            active_memories: active_memories as usize,
            expired_memories: expired_memories as usize,
            total_vectors: total_vectors as usize,
            by_category,
        })
    }
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryRecord> {
    let id: String = row.get(0)?;
    let category_str: String = row.get(1)?;
    let title: String = row.get(2)?;
    let content: String = row.get(3)?;
    let tags_str: String = row.get(4)?;
    let confidence: f64 = row.get(5)?;
    let status_str: String = row.get(6)?;
    let provenance: String = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let updated_at_str: String = row.get(9)?;
    let expired_at_str: Option<String> = row.get(10)?;

    let category = category_str.parse::<MemoryType>().unwrap_or(MemoryType::Fact);
    let status = status_str.parse::<MemoryStatus>().unwrap_or(MemoryStatus::Active);

    let tags = if tags_str.trim().is_empty() {
        Vec::new()
    } else {
        tags_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let expired_at = expired_at_str.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    });

    Ok(MemoryRecord {
        id,
        category,
        title,
        content,
        tags,
        confidence: confidence as f32,
        status,
        provenance,
        created_at,
        updated_at,
        expired_at,
    })
}

fn serialize_f32_slice(slice: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(slice.len() * 4);
    for &val in slice {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

fn deserialize_f32_slice(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().unwrap();
        out.push(f32::from_le_bytes(arr));
    }
    out
}

fn sanitize_fts5_query(query: &str) -> String {
    // Strip characters that break FTS5 grammar
    let mut tokens = Vec::new();
    for word in query.split_whitespace() {
        let clean: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !clean.is_empty() {
            tokens.push(format!("\"{}\"*", clean));
        }
    }
    tokens.join(" ")
}
