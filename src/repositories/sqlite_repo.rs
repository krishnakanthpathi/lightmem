use crate::models::{MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories, StorageStats};
use crate::services::embeddings::{validate_vector, EmbeddingProvider};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
        conn_settings(&storage)?;
        storage.migrate()?;
        Ok(storage)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        anyhow::ensure!(
            version <= 2,
            "Database schema is newer than this version of LightMem"
        );
        if version == 2 {
            return Ok(());
        }
        tx.execute_batch(
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

        // Upgrade legacy databases atomically and repair duplicate/stale FTS rows.
        tx.execute_batch("DELETE FROM memories_fts;
            INSERT INTO memories_fts(id, title, content, tags) SELECT id, title, content, tags FROM memories;
            CREATE TABLE IF NOT EXISTS embedding_state (singleton INTEGER PRIMARY KEY CHECK(singleton=1), identity TEXT NOT NULL, dims INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS storage_revision (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL);
            INSERT OR IGNORE INTO storage_revision VALUES (1, 0);
            CREATE TRIGGER IF NOT EXISTS revision_ai AFTER INSERT ON memories BEGIN UPDATE storage_revision SET revision=revision+1; END;
            CREATE TRIGGER IF NOT EXISTS revision_au AFTER UPDATE ON memories BEGIN UPDATE storage_revision SET revision=revision+1; END;
            CREATE TRIGGER IF NOT EXISTS revision_ad AFTER DELETE ON memories BEGIN UPDATE storage_revision SET revision=revision+1; END;")?;
        let has_key = {
            let mut stmt = tx.prepare("PRAGMA table_info(memories)")?;
            let names = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "dedup_key")
        };
        if !has_key {
            tx.execute(
                "ALTER TABLE memories ADD COLUMN dedup_key TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        tx.execute("CREATE INDEX IF NOT EXISTS idx_memories_dedup ON memories(status, category, dedup_key)", [])?;
        // Store tags as JSON so commas inside a tag survive a round trip.
        let tags: Vec<(String, String, String)> = {
            let mut stmt = tx.prepare("SELECT id, tags, content FROM memories")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, tags, content) in tags {
            let values: Vec<String> = tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect();
            tx.execute(
                "UPDATE memories SET tags=?1, dedup_key=?2 WHERE id=?3",
                params![serde_json::to_string(&values)?, duplicate_key(&content), id],
            )?;
        }
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_memory(&self, memory: &MemoryRecord, vector: Option<&[f32]>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // Low-level writes cannot prove the embedding model identity. Invalidate it.
        tx.execute("DELETE FROM embedding_state", [])?;
        write_record(&tx, memory, true, |merged| {
            if merged.to_card_text() != memory.to_card_text() {
                return Ok(None);
            }
            Ok(vector.map(<[f32]>::to_vec))
        })?;
        tx.commit()?;
        Ok(())
    }

    pub fn embedding_identity(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT identity FROM embedding_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn check_embedding_identity(&self, identity: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        check_identity(&conn, identity)
    }

    pub fn insert_indexed_batch(
        &self,
        memories: &[MemoryRecord],
        embedder: &dyn EmbeddingProvider,
        identity: &str,
        merge: bool,
    ) -> Result<Vec<MemoryRecord>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        check_identity(&tx, identity)?;
        let empty: bool = tx.query_row("SELECT NOT EXISTS(SELECT 1 FROM memories)", [], |r| {
            r.get(0)
        })?;
        if empty {
            tx.execute("DELETE FROM embedding_state", [])?;
        }
        let mut records = Vec::with_capacity(memories.len());
        for memory in memories {
            records.push(write_record(&tx, memory, merge, |_| Ok(None))?);
        }
        // Later rows in one import may merge away an earlier survivor. Resolve all
        // returned records to their final survivor before embedding the final cards.
        for record in &mut records {
            let exact = tx.query_row("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE id=?1", [&record.id], row_to_memory).optional()?;
            *record = match exact {
                Some(current) => current,
                None if merge && record.status == MemoryStatus::Active => tx.query_row("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE status='active' AND category=?1 AND dedup_key=?2 ORDER BY created_at ASC, id ASC LIMIT 1", params![record.category.as_str(), duplicate_key(&record.content)], row_to_memory)?,
                None => anyhow::bail!("An imported record unexpectedly disappeared"),
            };
        }
        let mut unique = std::collections::BTreeMap::new();
        for record in &records {
            unique.insert(record.id.clone(), record.clone());
        }
        let final_records: Vec<_> = unique.into_values().collect();
        for batch in final_records.chunks(32) {
            let texts: Vec<_> = batch.iter().map(MemoryRecord::to_card_text).collect();
            let vectors = embedder.embed_batch(&texts)?;
            anyhow::ensure!(
                vectors.len() == batch.len(),
                "Embedding provider returned the wrong batch size"
            );
            for (memory, vector) in batch.iter().zip(vectors) {
                validate_vector(&vector)?;
                bind_identity(&tx, identity, vector.len())?;
                tx.execute(
                    "INSERT INTO memory_vectors VALUES (?1, ?2, ?3)",
                    params![memory.id, serialize_f32_slice(&vector), vector.len() as i64],
                )?;
            }
        }
        tx.commit()?;
        Ok(records)
    }

    /// Snapshot memory rows and a monotonic revision together for optimistic index migration.
    pub fn index_snapshot(&self) -> Result<(i64, Vec<MemoryRecord>)> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let revision = tx.query_row("SELECT revision FROM storage_revision", [], |r| r.get(0))?;
        let records = all_records(&tx)?;
        tx.commit()?;
        Ok((revision, records))
    }

    pub fn replace_index(
        &self,
        revision: i64,
        identity: &str,
        vectors: &[(String, Vec<f32>)],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: i64 =
            tx.query_row("SELECT revision FROM storage_revision", [], |r| r.get(0))?;
        anyhow::ensure!(
            revision == current,
            "Memories changed during migration; old index preserved. Retry reindex."
        );
        let ids: std::collections::HashSet<String> =
            all_records(&tx)?.into_iter().map(|m| m.id).collect();
        let supplied: std::collections::HashSet<String> =
            vectors.iter().map(|(id, _)| id.clone()).collect();
        anyhow::ensure!(
            ids == supplied && ids.len() == vectors.len(),
            "Migration must cover every memory exactly once"
        );
        tx.execute("DELETE FROM memory_vectors", [])?;
        tx.execute("DELETE FROM embedding_state", [])?;
        for (id, vector) in vectors {
            validate_vector(vector)?;
            bind_identity(&tx, identity, vector.len())?;
            tx.execute(
                "INSERT INTO memory_vectors VALUES (?1, ?2, ?3)",
                params![id, serialize_f32_slice(vector), vector.len() as i64],
            )?;
        }
        // Empty databases will bind their dimensions on the first write.
        tx.execute("UPDATE storage_revision SET revision=revision+1", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn deduplicate_and_merge(&self) -> Result<usize> {
        self.deduplicate_with(None)
    }

    pub fn deduplicate_indexed(
        &self,
        embedder: &dyn EmbeddingProvider,
        identity: &str,
    ) -> Result<usize> {
        self.deduplicate_with(Some((embedder, identity)))
    }

    fn deduplicate_with(&self, indexed: Option<(&dyn EmbeddingProvider, &str)>) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some((_, identity)) = indexed {
            check_identity(&tx, identity)?;
        }
        let records = all_records(&tx)?;
        let mut groups: std::collections::BTreeMap<(String, String), Vec<MemoryRecord>> =
            std::collections::BTreeMap::new();
        for record in records
            .into_iter()
            .filter(|m| m.status == MemoryStatus::Active)
        {
            groups
                .entry((
                    record.category.as_str().into(),
                    duplicate_key(&record.content),
                ))
                .or_default()
                .push(record);
        }
        let mut removed = 0;
        for records in groups.into_values().filter(|g| g.len() > 1) {
            let primary = &records[0]; // all_records orders oldest first, with a stable ID tie-break.
            write_record(&tx, primary, true, |merged| {
                if let Some((embedder, identity)) = indexed {
                    let v = embedder.embed(&merged.to_card_text())?;
                    validate_vector(&v)?;
                    bind_identity(&tx, identity, v.len())?;
                    Ok(Some(v))
                } else {
                    Ok(None)
                }
            })?;
            removed += records.len() - 1;
        }
        if removed > 0 && indexed.is_none() {
            tx.execute("DELETE FROM embedding_state", [])?;
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn expire_due_memories(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        expire_due_conn(&conn)
    }

    pub fn get_memory(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let Some(id) = resolve_id(&conn, id)? else {
            return Ok(None);
        };
        Ok(conn.query_row("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE id=?1", [id], row_to_memory).optional()?)
    }

    pub fn get_memories_exact(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, MemoryRecord>> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let mut records = std::collections::HashMap::new();
        for chunk in ids.chunks(400) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE id IN ({})", placeholders);
            let mut stmt = conn.prepare(&sql)?;
            for row in stmt.query_map(rusqlite::params_from_iter(chunk), row_to_memory)? {
                let record = row?;
                records.insert(record.id.clone(), record);
            }
        }
        Ok(records)
    }

    pub fn forget_memory(&self, id: &str, hard_delete: bool) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(id) = resolve_id(&tx, id)? else {
            return Ok(false);
        };
        let rows = if hard_delete {
            tx.execute("DELETE FROM memories WHERE id=?1", [id])?
        } else {
            tx.execute("UPDATE memories SET status='expired', expired_at=?1, updated_at=?1 WHERE id=?2 AND status='active'", params![Utc::now().to_rfc3339(), id])?
        };
        tx.commit()?;
        Ok(rows > 0)
    }

    pub fn clear_all(&self, hard_delete: bool) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let rows = if hard_delete {
            let deleted = tx.execute("DELETE FROM memories", [])?;
            tx.execute("DELETE FROM embedding_state", [])?;
            deleted
        } else {
            tx.execute(
                "UPDATE memories SET status='expired', expired_at=?1, updated_at=?1 WHERE status='active'",
                params![Utc::now().to_rfc3339()],
            )?
        };
        tx.commit()?;
        Ok(rows)
    }

    pub fn count_memories(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let mut query = String::from("SELECT COUNT(*) FROM memories WHERE 1=1");
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
            query.push_str(&format!(
                " AND (expired_at IS NULL OR expired_at > ?{})",
                params_vec.len()
            ));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            query.push_str(&format!(" AND status = ?{}", params_vec.len()));
        }

        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let total: i64 = conn.query_row(&query, param_refs.as_slice(), |r| r.get(0))?;
        Ok(total as usize)
    }

    pub fn list_memories_paginated(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedMemories> {
        let total = self.count_memories(category, status, as_of)?;

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
            query.push_str(&format!(
                " AND (expired_at IS NULL OR expired_at > ?{})",
                params_vec.len()
            ));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            query.push_str(&format!(" AND status = ?{}", params_vec.len()));
        }

        query.push_str(" ORDER BY created_at DESC");

        if limit > 0 {
            params_vec.push(Box::new(limit as i64));
            query.push_str(&format!(" LIMIT ?{}", params_vec.len()));
            params_vec.push(Box::new(offset as i64));
            query.push_str(&format!(" OFFSET ?{}", params_vec.len()));
        } else if offset > 0 {
            params_vec.push(Box::new(-1i64));
            query.push_str(&format!(" LIMIT ?{}", params_vec.len()));
            params_vec.push(Box::new(offset as i64));
            query.push_str(&format!(" OFFSET ?{}", params_vec.len()));
        }

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), row_to_memory)?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }

        let effective_limit = if limit == 0 { total.max(1) } else { limit };
        let page = (offset / effective_limit) + 1;
        let total_pages = if total == 0 {
            1
        } else {
            total.div_ceil(effective_limit)
        };
        let has_more = offset + items.len() < total;

        Ok(PaginatedMemories {
            items,
            total,
            limit,
            offset,
            page,
            total_pages,
            has_more,
        })
    }

    pub fn list_memories(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        Ok(self
            .list_memories_paginated(category, status, as_of, limit, 0)?
            .items)
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
        let _ = expire_due_conn(&conn);
        let mut sql = String::from(
            r#"
            SELECT m.id, bm25(memories_fts) as rank
            FROM memories_fts f
            JOIN memories m ON m.id = f.id
            WHERE memories_fts MATCH ?1
            "#,
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
            sql.push_str(&format!(
                " AND ((m.status = 'active' AND m.expired_at IS NULL) OR (m.expired_at IS NOT NULL AND m.expired_at > ?{}))",
                params_vec.len()
            ));
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
        self.get_candidate_vectors_checked(category, status, as_of, None)
    }

    pub fn get_candidate_vectors_checked(
        &self,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        identity: Option<&str>,
    ) -> Result<Vec<(String, Vec<f32>)>> {
        let mut connection = self.conn.lock().unwrap();
        let _ = expire_due_conn(&connection);
        let snapshot = connection.transaction()?;
        let conn = &snapshot;
        if let Some(identity) = identity {
            check_identity(conn, identity)?;
        }
        let mut sql = String::from(
            r#"
            SELECT v.id, v.embedding, v.dims
            FROM memory_vectors v
            JOIN memories m ON m.id = v.id
            WHERE 1=1
            "#,
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
            sql.push_str(&format!(
                " AND ((m.status = 'active' AND m.expired_at IS NULL) OR (m.expired_at IS NOT NULL AND m.expired_at > ?{}))",
                params_vec.len()
            ));
        } else if let Some(st) = status {
            params_vec.push(Box::new(st.as_str().to_string()));
            sql.push_str(&format!(" AND m.status = ?{}", params_vec.len()));
        }

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dims: i64 = row.get(2)?;
            if dims <= 0 || !blob.len().is_multiple_of(4) || dims as usize != blob.len() / 4 {
                return Err(rusqlite::Error::InvalidQuery);
            }
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
        let _ = expire_due_conn(&conn);

        let total_memories: i64 =
            conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        let active_memories: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE status = 'active'",
            [],
            |r| r.get(0),
        )?;
        let expired_memories: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE status = 'expired'",
            [],
            |r| r.get(0),
        )?;
        let total_vectors: i64 =
            conn.query_row("SELECT COUNT(*) FROM memory_vectors", [], |r| r.get(0))?;

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) FROM memories GROUP BY category ORDER BY COUNT(*) DESC",
        )?;
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

fn expire_due_conn(conn: &Connection) -> Result<usize> {
    let now = Utc::now().to_rfc3339();
    let has_due: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE status='active' AND expired_at IS NOT NULL AND expired_at <= ?1)",
        [&now],
        |r| r.get(0),
    )?;
    if !has_due {
        return Ok(0);
    }
    let expired = conn.execute(
        "UPDATE memories SET status='expired', updated_at=?1 WHERE status='active' AND expired_at IS NOT NULL AND expired_at <= ?1",
        [&now],
    )?;
    Ok(expired)
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

    let category = category_str
        .parse::<MemoryType>()
        .unwrap_or(MemoryType::Fact);
    let status = status_str
        .parse::<MemoryStatus>()
        .unwrap_or(MemoryStatus::Active);

    let tags: Vec<String> = serde_json::from_str(&tags_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;

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
    let (chunks, _) = bytes.as_chunks::<4>();
    for &chunk in chunks {
        out.push(f32::from_le_bytes(chunk));
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

fn conn_settings(storage: &Storage) -> Result<()> {
    storage
        .conn
        .lock()
        .unwrap()
        .pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Only trim outer whitespace. Case, code indentation, and secrets remain significant.
pub fn duplicate_key(content: &str) -> String {
    content.trim().to_string()
}

fn all_records(conn: &Connection) -> Result<Vec<MemoryRecord>> {
    let mut stmt = conn.prepare("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories ORDER BY created_at ASC, id ASC")?;
    let rows = stmt.query_map([], row_to_memory)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn resolve_id(conn: &Connection, id: &str) -> Result<Option<String>> {
    anyhow::ensure!(!id.trim().is_empty(), "Memory ID cannot be empty");
    if let Some(exact) = conn
        .query_row("SELECT id FROM memories WHERE id=?1", [id], |r| r.get(0))
        .optional()?
    {
        return Ok(Some(exact));
    }
    let mut stmt = conn.prepare(
        "SELECT id FROM memories WHERE substr(id, 1, length(?1)) = ?1 ORDER BY id LIMIT 2",
    )?;
    let ids = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    anyhow::ensure!(
        ids.len() <= 1,
        "Ambiguous memory ID prefix '{}'; use a full ID",
        id
    );
    Ok(ids.into_iter().next())
}

fn check_identity(conn: &Connection, identity: &str) -> Result<()> {
    let state: Option<String> = conn
        .query_row("SELECT identity FROM embedding_state", [], |r| r.get(0))
        .optional()?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    anyhow::ensure!(count == 0 || state.as_deref() == Some(identity), "Embedding migration required (stored: {}, requested: {}). Run lmem reindex to review and start migration.", state.as_deref().unwrap_or("legacy / unknown"), identity);
    Ok(())
}

fn bind_identity(conn: &Connection, identity: &str, dims: usize) -> Result<()> {
    let state: Option<(String, i64)> = conn
        .query_row("SELECT identity, dims FROM embedding_state", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    if let Some((stored, size)) = state {
        anyhow::ensure!(
            stored == identity && size == dims as i64,
            "Embedding model or dimensions changed; run lmem reindex"
        );
    } else {
        conn.execute(
            "INSERT INTO embedding_state VALUES (1, ?1, ?2)",
            params![identity, dims as i64],
        )?;
    }
    Ok(())
}

fn write_record<F>(
    tx: &Transaction<'_>,
    memory: &MemoryRecord,
    merge: bool,
    embed: F,
) -> Result<MemoryRecord>
where
    F: FnOnce(&MemoryRecord) -> Result<Option<Vec<f32>>>,
{
    anyhow::ensure!(
        !memory.id.trim().is_empty() && !memory.content.trim().is_empty(),
        "Memory ID and content cannot be blank"
    );
    anyhow::ensure!(
        memory.confidence.is_finite() && (0.0..=1.0).contains(&memory.confidence),
        "Confidence must be between 0 and 1"
    );
    let mut merged = memory.clone();
    let duplicates: Vec<MemoryRecord> = if merge && memory.status == MemoryStatus::Active {
        let mut stmt = tx.prepare("SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE status='active' AND category=?1 AND dedup_key=?2 ORDER BY created_at ASC, id ASC")?;
        let rows = stmt.query_map(
            params![memory.category.as_str(), duplicate_key(&memory.content)],
            row_to_memory,
        )?;
        rows.collect::<rusqlite::Result<_>>()?
    } else {
        Vec::new()
    };
    let exact: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id=?1)",
        [&memory.id],
        |r| r.get(0),
    )?;
    if !exact {
        if let Some(first) = duplicates.first() {
            merged.id = first.id.clone();
            merged.provenance = first.provenance.clone();
        }
    }
    for existing in &duplicates {
        merged.confidence = merged.confidence.max(existing.confidence);
        merged.created_at = merged.created_at.min(existing.created_at);
        merged.updated_at = merged.updated_at.max(existing.updated_at);
        merged.expired_at = merged.expired_at.or(existing.expired_at);
        let auto = |m: &MemoryRecord| {
            m.title
                == m.content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect::<String>()
                || m.title.ends_with("...")
        };
        if auto(&merged) && !auto(existing) {
            merged.title = existing.title.clone();
        }
        for tag in &existing.tags {
            if !merged.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                merged.tags.push(tag.clone());
            }
        }
    }
    let vector = embed(&merged)?;
    tx.execute("INSERT INTO memories (id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at, dedup_key)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
        ON CONFLICT(id) DO UPDATE SET category=excluded.category, title=excluded.title, content=excluded.content, tags=excluded.tags, confidence=excluded.confidence, status=excluded.status, provenance=excluded.provenance, created_at=excluded.created_at, updated_at=excluded.updated_at, expired_at=excluded.expired_at, dedup_key=excluded.dedup_key",
        params![merged.id, merged.category.as_str(), merged.title, merged.content, serde_json::to_string(&merged.tags)?, merged.confidence, merged.status.as_str(), merged.provenance, merged.created_at.to_rfc3339(), merged.updated_at.to_rfc3339(), merged.expired_at.map(|t| t.to_rfc3339()), duplicate_key(&merged.content)])?;
    tx.execute("DELETE FROM memory_vectors WHERE id=?1", [&merged.id])?;
    if let Some(vector) = vector {
        validate_vector(&vector)?;
        tx.execute(
            "INSERT INTO memory_vectors VALUES (?1,?2,?3)",
            params![merged.id, serialize_f32_slice(&vector), vector.len() as i64],
        )?;
    }
    for duplicate in duplicates {
        if duplicate.id != merged.id {
            tx.execute("DELETE FROM memories WHERE id=?1", [duplicate.id])?;
        }
    }
    Ok(merged)
}
