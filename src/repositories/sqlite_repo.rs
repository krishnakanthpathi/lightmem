use crate::models::{
    extract_wikilinks, GraphEdge, GraphNode, GraphSnapshot, MemoryLink, MemoryRecord, MemoryStatus,
    MemoryType, PaginatedMemories, RelatedMemory, StorageStats,
};
use crate::services::embeddings::{cosine_similarity, validate_vector, EmbeddingProvider};
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
            version <= 3,
            "Database schema is newer than this version of LightMem"
        );
        if version == 3 {
            let needs_tag_or_key_repair: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM memories WHERE tags NOT LIKE '[%' OR (dedup_key = '' AND TRIM(content) != ''))",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if needs_tag_or_key_repair {
                repair_legacy_tags_and_dedup_keys(&tx)?;
                tx.commit()?;
            }
            return Ok(());
        }

        if version < 2 {
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
            repair_legacy_tags_and_dedup_keys(&tx)?;
        } else {
            let needs_tag_or_key_repair: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM memories WHERE tags NOT LIKE '[%' OR (dedup_key = '' AND TRIM(content) != ''))",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if needs_tag_or_key_repair {
                repair_legacy_tags_and_dedup_keys(&tx)?;
            }
        }

        // Schema v3: memory_links table
        tx.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memory_links (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation TEXT NOT NULL DEFAULT 'relates_to',
                weight REAL NOT NULL DEFAULT 1.0,
                created_at TEXT NOT NULL,
                PRIMARY KEY (source_id, target_id, relation),
                FOREIGN KEY (source_id) REFERENCES memories(id) ON DELETE CASCADE,
                FOREIGN KEY (target_id) REFERENCES memories(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_links_source ON memory_links(source_id);
            CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id);
            "#,
        )?;
        tx.pragma_update(None, "user_version", 3)?;
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

    pub fn get_memory_by_id_or_title(&self, term: &str) -> Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let Some(id) = resolve_id_or_title(&conn, term)? else {
            return Ok(None);
        };
        Ok(conn
            .query_row(
                "SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE id=?1",
                [id],
                row_to_memory,
            )
            .optional()?)
    }

    pub fn add_link(
        &self,
        source_term: &str,
        target_term: &str,
        relation: Option<&str>,
        weight: Option<f32>,
    ) -> Result<(String, String)> {
        let conn = self.conn.lock().unwrap();
        let src = resolve_id_or_title(&conn, source_term)?
            .with_context(|| format!("Source memory '{}' not found", source_term))?;
        let dst = resolve_id_or_title(&conn, target_term)?
            .with_context(|| format!("Target memory '{}' not found", target_term))?;
        anyhow::ensure!(src != dst, "Cannot link a memory to itself");

        let rel = relation
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .unwrap_or("relates_to");
        let w = weight.unwrap_or(1.0).clamp(0.01, 10.0);
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memory_links (source_id, target_id, relation, weight, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_id, target_id, relation) DO UPDATE SET weight=?4",
            params![src, dst, rel, w, now],
        )?;

        Ok((src, dst))
    }

    pub fn remove_link(
        &self,
        source_term: &str,
        target_term: &str,
        relation: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let src = resolve_id_or_title(&conn, source_term)?
            .with_context(|| format!("Source memory '{}' not found", source_term))?;
        let dst = resolve_id_or_title(&conn, target_term)?
            .with_context(|| format!("Target memory '{}' not found", target_term))?;

        let rows = if let Some(rel) = relation.map(|r| r.trim()).filter(|r| !r.is_empty()) {
            conn.execute(
                "DELETE FROM memory_links WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3",
                params![src, dst, rel],
            )?
        } else {
            conn.execute(
                "DELETE FROM memory_links WHERE (source_id = ?1 AND target_id = ?2) OR (source_id = ?2 AND target_id = ?1)",
                params![src, dst],
            )?
        };

        Ok(rows > 0)
    }

    pub fn get_links_for_memory(&self, memory_term: &str) -> Result<Vec<MemoryLink>> {
        let conn = self.conn.lock().unwrap();
        let id = resolve_id_or_title(&conn, memory_term)?
            .with_context(|| format!("Memory '{}' not found", memory_term))?;
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, relation, weight, created_at
             FROM memory_links
             WHERE source_id = ?1 OR target_id = ?1
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([id], |r| {
            let created_at_str: String = r.get(4)?;
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(MemoryLink {
                source_id: r.get(0)?,
                target_id: r.get(1)?,
                relation: r.get(2)?,
                weight: r.get::<_, f64>(3)? as f32,
                created_at,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn traverse_multi_hop(
        &self,
        start_term: &str,
        max_hops: usize,
    ) -> Result<Vec<RelatedMemory>> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let Some(start_id) = resolve_id_or_title(&conn, start_term)? else {
            return Ok(Vec::new());
        };

        let max_hops = max_hops.clamp(1, 10);
        let mut visited = std::collections::HashSet::new();
        visited.insert(start_id.clone());

        // Queue: (node_id, current_hop, relation_path, cumulative_score)
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((start_id, 0usize, Vec::<String>::new(), 1.0f32));

        let mut results = Vec::new();

        while let Some((curr_id, curr_hop, path, score)) = queue.pop_front() {
            if curr_hop >= max_hops {
                continue;
            }

            let mut stmt = conn.prepare(
                r#"
                SELECT m.id, l.relation, l.weight
                FROM memory_links l
                JOIN memories m ON l.target_id = m.id
                WHERE l.source_id = ?1 AND m.status = 'active'
                UNION ALL
                SELECT m.id, l.relation || ' (incoming)' as relation, l.weight
                FROM memory_links l
                JOIN memories m ON l.source_id = m.id
                WHERE l.target_id = ?1 AND m.status = 'active'
                "#,
            )?;

            let neighbors = stmt
                .query_map([&curr_id], |r| {
                    let neighbor_id: String = r.get(0)?;
                    let relation: String = r.get(1)?;
                    let weight: f64 = r.get(2)?;
                    Ok((neighbor_id, relation, weight as f32))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (neighbor_id, relation, weight) in neighbors {
                if visited.contains(&neighbor_id) {
                    continue;
                }
                visited.insert(neighbor_id.clone());

                let next_hop = curr_hop + 1;
                let mut next_path = path.clone();
                next_path.push(relation);
                let next_score = (score * weight) / (next_hop as f32);

                if let Some(mem) = conn
                    .query_row(
                        "SELECT id, category, title, content, tags, confidence, status, provenance, created_at, updated_at, expired_at FROM memories WHERE id=?1",
                        [&neighbor_id],
                        row_to_memory,
                    )
                    .optional()?
                {
                    results.push(RelatedMemory {
                        memory: mem,
                        distance: next_hop,
                        relation_path: next_path.clone(),
                        score: next_score,
                    });
                }

                if next_hop < max_hops {
                    queue.push_back((neighbor_id, next_hop, next_path, next_score));
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    pub fn get_graph_snapshot(
        &self,
        focus_term: Option<&str>,
        max_hops: Option<usize>,
    ) -> Result<GraphSnapshot> {
        let conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);

        let allowed_ids: Option<std::collections::HashSet<String>> = if let Some(focus) = focus_term {
            if let Some(start_id) = resolve_id_or_title(&conn, focus)? {
                let hops = max_hops.unwrap_or(2).clamp(1, 10);
                let mut set = std::collections::HashSet::new();
                set.insert(start_id.clone());
                let mut queue = std::collections::VecDeque::new();
                queue.push_back((start_id, 0usize));
                while let Some((curr, h)) = queue.pop_front() {
                    if h >= hops {
                        continue;
                    }
                    let mut stmt = conn.prepare(
                        r#"
                        SELECT target_id FROM memory_links WHERE source_id = ?1
                        UNION
                        SELECT source_id FROM memory_links WHERE target_id = ?1
                        "#,
                    )?;
                    let nbrs = stmt
                        .query_map([&curr], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    for nbr in nbrs {
                        if set.insert(nbr.clone()) && h + 1 < hops {
                            queue.push_back((nbr, h + 1));
                        }
                    }
                }
                Some(set)
            } else {
                return Ok(GraphSnapshot {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                });
            }
        } else {
            None
        };

        let mut stmt = conn.prepare(
            "SELECT id, title, category, tags, confidence, content FROM memories WHERE status = 'active'",
        )?;
        let mut nodes_map = std::collections::HashMap::new();
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let title: String = r.get(1)?;
            let category: String = r.get(2)?;
            let tags_str: String = r.get(3)?;
            let confidence: f64 = r.get(4)?;
            let content: String = r.get(5)?;
            let tags = parse_tags_column(&tags_str);
            let snippet = if content.chars().count() > 140 {
                format!("{}...", content.chars().take(140).collect::<String>())
            } else {
                content.clone()
            };
            Ok((
                id.clone(),
                GraphNode {
                    id,
                    label: if title.trim().is_empty() {
                        content.chars().take(30).collect()
                    } else {
                        title
                    },
                    category,
                    tags,
                    confidence: confidence as f32,
                    degree: 0,
                    snippet,
                },
            ))
        })?;

        for r in rows {
            let (id, node) = r?;
            if let Some(ref allowed) = allowed_ids {
                if !allowed.contains(&id) {
                    continue;
                }
            }
            nodes_map.insert(id, node);
        }

        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, relation, weight FROM memory_links",
        )?;
        let mut edges = Vec::new();
        let edge_rows = stmt.query_map([], |r| {
            Ok(GraphEdge {
                source: r.get(0)?,
                target: r.get(1)?,
                relation: r.get(2)?,
                weight: r.get::<_, f64>(3)? as f32,
            })
        })?;

        for edge_res in edge_rows {
            let edge = edge_res?;
            if nodes_map.contains_key(&edge.source) && nodes_map.contains_key(&edge.target) {
                if let Some(src_node) = nodes_map.get_mut(&edge.source) {
                    src_node.degree += 1;
                }
                if let Some(dst_node) = nodes_map.get_mut(&edge.target) {
                    dst_node.degree += 1;
                }
                edges.push(edge);
            }
        }

        let nodes = nodes_map.into_values().collect();
        Ok(GraphSnapshot { nodes, edges })
    }

    pub fn auto_link_memory(&self, memory_id: &str, text: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let Some(source_id) = resolve_id(&conn, memory_id)? else {
            return Ok(0);
        };

        let mut linked = 0;
        let wikilinks = extract_wikilinks(text);
        let now = Utc::now().to_rfc3339();

        for target in wikilinks {
            if let Some(target_id) = resolve_id_or_title(&conn, &target)? {
                if target_id != source_id {
                    let rows = conn.execute(
                        "INSERT OR IGNORE INTO memory_links (source_id, target_id, relation, weight, created_at)
                         VALUES (?1, ?2, 'references', 1.0, ?3)",
                        params![source_id, target_id, now],
                    )?;
                    linked += rows;
                }
            }
        }

        // Also check if any existing active memories have wikilinks pointing to this memory's title or id
        if let Some(title) = conn
            .query_row(
                "SELECT title FROM memories WHERE id = ?1",
                [&source_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            let clean_title = title.trim();
            if !clean_title.is_empty() {
                let pattern = format!("%[[{}]]%", clean_title);
                let mut stmt = conn.prepare(
                    "SELECT id FROM memories WHERE content LIKE ?1 AND status = 'active' AND id != ?2",
                )?;
                let backward_ids = stmt
                    .query_map(params![pattern, source_id], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                for back_id in backward_ids {
                    let rows = conn.execute(
                        "INSERT OR IGNORE INTO memory_links (source_id, target_id, relation, weight, created_at)
                         VALUES (?1, ?2, 'references', 1.0, ?3)",
                        params![back_id, source_id, now],
                    )?;
                    linked += rows;
                }
            }
        }

        Ok(linked)
    }

    pub fn autolink_vault(&self, min_similarity: f32) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let _ = expire_due_conn(&conn);
        let now = Utc::now().to_rfc3339();

        struct Candidate {
            id: String,
            title: String,
            content_lower: String,
            tags: Vec<String>,
        }

        let mut stmt = conn.prepare(
            "SELECT id, title, content, tags FROM memories WHERE status = 'active'",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let title: String = r.get(1)?;
            let content: String = r.get(2)?;
            let tags_str: String = r.get(3)?;
            Ok(Candidate {
                id,
                title: title.trim().to_string(),
                content_lower: content.to_lowercase(),
                tags: parse_tags_column(&tags_str),
            })
        })?;
        let candidates: Vec<Candidate> = rows.collect::<rusqlite::Result<_>>()?;

        let mut vec_stmt = conn.prepare(
            "SELECT v.id, v.embedding FROM memory_vectors v JOIN memories m ON v.id = m.id WHERE m.status = 'active'",
        )?;
        let mut vectors: std::collections::HashMap<String, Vec<f32>> = std::collections::HashMap::new();
        let vec_rows = vec_stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            Ok((id, deserialize_f32_slice(&blob)))
        })?;
        for item in vec_rows {
            let (id, vec) = item?;
            vectors.insert(id, vec);
        }
        drop(stmt);
        drop(vec_stmt);

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut insert_stmt = tx.prepare(
            "INSERT OR IGNORE INTO memory_links (source_id, target_id, relation, weight, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut total_created = 0;

        const GENERIC_TITLES: &[&str] = &[
            "untitled", "note", "fact", "decision", "memory", "scratch", "test", "todo", "temp",
        ];
        const GENERIC_TAGS: &[&str] = &[
            "fact", "decision", "imported", "auto", "default", "quick", "general", "temp",
        ];

        // 1. Title mentions
        for b in &candidates {
            let b_title_clean = b.title.trim();
            if b_title_clean.chars().count() >= 4 {
                let b_title_lower = b_title_clean.to_lowercase();
                if !GENERIC_TITLES.contains(&b_title_lower.as_str()) {
                    for a in &candidates {
                        if a.id != b.id && a.content_lower.contains(&b_title_lower) {
                            if let Some(idx) = a.content_lower.find(&b_title_lower) {
                                let before_ok = if idx == 0 {
                                    true
                                } else {
                                    let prev = a.content_lower.as_bytes()[idx - 1] as char;
                                    !prev.is_alphanumeric()
                                };
                                let after_idx = idx + b_title_lower.len();
                                let after_ok = if after_idx >= a.content_lower.len() {
                                    true
                                } else {
                                    let next = a.content_lower.as_bytes()[after_idx] as char;
                                    !next.is_alphanumeric()
                                };
                                if before_ok && after_ok {
                                    total_created += insert_stmt.execute(params![
                                        &a.id,
                                        &b.id,
                                        "mentions",
                                        1.0f32,
                                        &now
                                    ])?;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Shared non-generic tags (>= 2)
        for i in 0..candidates.len() {
            let a = &candidates[i];
            let a_tags: std::collections::HashSet<String> = a
                .tags
                .iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty() && !GENERIC_TAGS.contains(&t.as_str()))
                .collect();
            if a_tags.is_empty() {
                continue;
            }

            for j in (i + 1)..candidates.len() {
                let b = &candidates[j];
                let shared_count = b
                    .tags
                    .iter()
                    .map(|t| t.trim().to_lowercase())
                    .filter(|t| a_tags.contains(t))
                    .count();

                if shared_count >= 2 {
                    total_created += insert_stmt.execute(params![
                        &a.id,
                        &b.id,
                        "shared_topic",
                        0.8f32,
                        &now
                    ])?;
                    total_created += insert_stmt.execute(params![
                        &b.id,
                        &a.id,
                        "shared_topic",
                        0.8f32,
                        &now
                    ])?;
                }
            }
        }

        // 3. Semantic vector cosine similarity (>= min_similarity)
        for i in 0..candidates.len() {
            let a_id = &candidates[i].id;
            let Some(a_vec) = vectors.get(a_id) else {
                continue;
            };

            for j in (i + 1)..candidates.len() {
                let b_id = &candidates[j].id;
                let Some(b_vec) = vectors.get(b_id) else {
                    continue;
                };

                let sim = cosine_similarity(a_vec, b_vec);
                if sim >= min_similarity {
                    let weight = ((sim * 100.0).round() / 100.0).clamp(0.1, 1.0);
                    total_created += insert_stmt.execute(params![
                        a_id,
                        b_id,
                        "relates_to",
                        weight,
                        &now
                    ])?;
                    total_created += insert_stmt.execute(params![
                        b_id,
                        a_id,
                        "relates_to",
                        weight,
                        &now
                    ])?;
                }
            }
        }

        drop(insert_stmt);
        tx.commit()?;
        Ok(total_created)
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
        drop(param_refs);
        if results.is_empty() {
            let clean_fallback = sanitize_fts5_query(query);
            let tokens: Vec<&str> = clean_fallback.split_whitespace().collect();
            if (3..=6).contains(&tokens.len()) {
                let mut pairs = Vec::new();
                for i in 0..tokens.len() {
                    for j in (i + 1)..tokens.len() {
                        pairs.push(format!("({} {})", tokens[i], tokens[j]));
                    }
                }
                params_vec[0] = Box::new(pairs.join(" OR "));
                let fallback_refs: Vec<&dyn rusqlite::ToSql> =
                    params_vec.iter().map(|p| p.as_ref()).collect();
                let fallback_rows = stmt.query_map(fallback_refs.as_slice(), |row| {
                    let id: String = row.get(0)?;
                    let rank: f64 = row.get(1)?;
                    Ok((id, rank as f32))
                })?;
                for r in fallback_rows {
                    results.push(r?);
                }
            }
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

fn parse_tags_column(tags_str: &str) -> Vec<String> {
    let trimmed = tags_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(trimmed) {
        return parsed;
    }
    trimmed
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

fn repair_legacy_tags_and_dedup_keys(tx: &Transaction) -> Result<()> {
    let rows_to_fix: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare("SELECT id, tags, content FROM memories")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        mapped.collect::<rusqlite::Result<_>>()?
    };
    for (id, tags, content) in rows_to_fix {
        let values = parse_tags_column(&tags);
        tx.execute(
            "UPDATE memories SET tags=?1, dedup_key=?2 WHERE id=?3",
            params![serde_json::to_string(&values)?, duplicate_key(&content), id],
        )?;
    }
    Ok(())
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

    let tags: Vec<String> = parse_tags_column(&tags_str);

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
    const STOPWORDS: &[&str] = &[
        "what", "which", "who", "where", "when", "why", "how", "is", "are", "was", "were", "does",
        "do", "did", "the", "a", "an", "in", "on", "at", "to", "for", "of", "with", "by", "from",
        "as", "and", "or", "my", "me", "our", "your", "use", "uses", "used", "run", "runs",
        "running", "please", "tell",
    ];

    let raw_tokens: Vec<&str> = query
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .map(|w| w.trim_matches(|c: char| c == '-' || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();

    let filtered: Vec<&str> = raw_tokens
        .iter()
        .copied()
        .filter(|t| !STOPWORDS.contains(&t.to_ascii_lowercase().as_str()))
        .collect();

    let active_tokens = if filtered.is_empty() {
        raw_tokens
    } else {
        filtered
    };

    active_tokens
        .into_iter()
        .map(|clean| format!("\"{}\"*", clean))
        .collect::<Vec<_>>()
        .join(" ")
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

fn resolve_id_or_title(conn: &Connection, term: &str) -> Result<Option<String>> {
    let clean = term.trim();
    if clean.is_empty() {
        return Ok(None);
    }
    // 1. Try resolve_id (exact UUID or unique ID prefix)
    if let Ok(Some(id)) = resolve_id(conn, clean) {
        return Ok(Some(id));
    }
    // 2. Exact title match (case-insensitive)
    let exact_title: Option<String> = conn
        .query_row(
            "SELECT id FROM memories WHERE LOWER(title) = LOWER(?1) AND status = 'active' LIMIT 1",
            [clean],
            |r| r.get(0),
        )
        .optional()?;
    if exact_title.is_some() {
        return Ok(exact_title);
    }
    // 3. Title substring match if term is at least 3 chars
    if clean.len() >= 3 {
        let pattern = format!("%{}%", clean);
        let mut stmt = conn.prepare(
            "SELECT id FROM memories WHERE LOWER(title) LIKE LOWER(?1) AND status = 'active' LIMIT 2",
        )?;
        let ids = stmt
            .query_map([&pattern], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() == 1 {
            return Ok(ids.into_iter().next());
        }
    }
    Ok(None)
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

#[cfg(test)]
mod fts_tests {
    use super::*;

    #[test]
    fn test_sanitize_fts5_query_splits_dotted_tokens_and_ips() {
        assert_eq!(
            sanitize_fts5_query("198.51.100.214"),
            "\"198\"* \"51\"* \"100\"* \"214\"*"
        );
        assert_eq!(sanitize_fts5_query("16.4.2-r9"), "\"16\"* \"4\"* \"2-r9\"*");
    }

    #[test]
    fn test_sanitize_fts5_query_filters_question_stopwords() {
        assert_eq!(
            sanitize_fts5_query("What port does Redis use?"),
            "\"port\"* \"Redis\"*"
        );
        // All-stopword query retains original tokens instead of returning empty
        assert_eq!(
            sanitize_fts5_query("what is the"),
            "\"what\"* \"is\"* \"the\"*"
        );
    }

    #[test]
    fn test_search_bm25_matches_dotted_versions_ips_and_natural_questions() {
        let storage = Storage::open_in_memory().unwrap();

        let mem_redis = MemoryRecord::new(
            MemoryType::Fact,
            "Redis port".to_string(),
            "Redis runs on port 6379 on Linux.".to_string(),
            vec!["redis".to_string()],
            0.9,
            None,
        );
        let mem_pg = MemoryRecord::new(
            MemoryType::Fact,
            "Postgres version".to_string(),
            "Production database is pinned to PostgreSQL 16.4.2-r9 on staging.".to_string(),
            vec!["postgres".to_string()],
            0.9,
            None,
        );
        let mem_ip = MemoryRecord::new(
            MemoryType::Fact,
            "Bastion host".to_string(),
            "Primary bastion host IP is 198.51.100.214:2222.".to_string(),
            vec!["network".to_string()],
            0.9,
            None,
        );

        storage.insert_memory(&mem_redis, None).unwrap();
        storage.insert_memory(&mem_pg, None).unwrap();
        storage.insert_memory(&mem_ip, None).unwrap();

        // Dotted version query
        let pg_hits = storage
            .search_bm25("16.4.2-r9", None, Some(MemoryStatus::Active), None, 10)
            .unwrap();
        assert_eq!(pg_hits.len(), 1);
        assert_eq!(pg_hits[0].0, mem_pg.id);

        // Dotted IP query
        let ip_hits = storage
            .search_bm25("198.51.100.214", None, Some(MemoryStatus::Active), None, 10)
            .unwrap();
        assert_eq!(ip_hits.len(), 1);
        assert_eq!(ip_hits[0].0, mem_ip.id);

        // Natural question with stopwords
        let q1_hits = storage
            .search_bm25(
                "What port does Redis use?",
                None,
                Some(MemoryStatus::Active),
                None,
                10,
            )
            .unwrap();
        assert!(!q1_hits.is_empty());
        assert_eq!(q1_hits[0].0, mem_redis.id);

        // Natural question requiring OR fallback ("service" is not in the memory)
        let q2_hits = storage
            .search_bm25(
                "What service runs on port 6379?",
                None,
                Some(MemoryStatus::Active),
                None,
                10,
            )
            .unwrap();
        assert!(!q2_hits.is_empty());
        assert_eq!(q2_hits[0].0, mem_redis.id);
    }

    #[test]
    fn test_legacy_comma_tags_with_user_version_2_repairs_and_lists_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memories.db");
        {
            let storage = Storage::open(&db_path).unwrap();
            let m = MemoryRecord::new(
                MemoryType::Fact,
                "HP Profile".to_string(),
                "Developer profile on kk-Linux".to_string(),
                vec!["json-tag".to_string()],
                0.9,
                None,
            );
            storage.insert_memory(&m, None).unwrap();
        }
        // Simulate ssh hp state: user_version is already 2, but a row has legacy comma-separated tags
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "UPDATE memories SET tags = 'profile,bio,career,developer,agent:hp-docker-ollama-moorcheh'",
                [],
            )
            .unwrap();
        }
        let reopened = Storage::open(&db_path).unwrap();
        let listed = reopened
            .list_memories(None, Some(MemoryStatus::Active), None, 10)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].tags,
            vec![
                "profile",
                "bio",
                "career",
                "developer",
                "agent:hp-docker-ollama-moorcheh"
            ]
        );
    }

    #[test]
    fn test_memory_linking_and_multihop() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_links.db");
        let storage = Storage::open(&db_path).unwrap();

        // 1. Create memories: A -> B -> C
        let mem_a = MemoryRecord::new(
            MemoryType::Decision,
            "Service Gateway".to_string(),
            "API Gateway routes to [[PostgreSQL DB]]".to_string(),
            vec!["infra".to_string()],
            0.9,
            None,
        );
        let mem_b = MemoryRecord::new(
            MemoryType::Artifact,
            "PostgreSQL DB".to_string(),
            "Postgres primary runs on port 5432 and stores data on [[NVMe Volume]]".to_string(),
            vec!["db".to_string()],
            0.95,
            None,
        );
        let mem_c = MemoryRecord::new(
            MemoryType::Fact,
            "NVMe Volume".to_string(),
            "NVMe mount located at /mnt/fast-storage".to_string(),
            vec!["storage".to_string()],
            0.85,
            None,
        );

        storage.insert_memory(&mem_a, None).unwrap();
        storage.insert_memory(&mem_b, None).unwrap();
        storage.insert_memory(&mem_c, None).unwrap();

        // Test auto_link_memory via wikilinks
        let linked_a = storage.auto_link_memory(&mem_a.id, &mem_a.content).unwrap();
        assert_eq!(linked_a, 1);
        let linked_b = storage.auto_link_memory(&mem_b.id, &mem_b.content).unwrap();
        assert_eq!(linked_b, 1);

        // Verify direct links for mem_a
        let links_a = storage.get_links_for_memory(&mem_a.id).unwrap();
        assert_eq!(links_a.len(), 1);
        assert_eq!(links_a[0].source_id, mem_a.id);
        assert_eq!(links_a[0].target_id, mem_b.id);
        assert_eq!(links_a[0].relation, "references");

        // Verify multi-hop traversal from A with 2 hops
        let related_2hops = storage.traverse_multi_hop(&mem_a.id, 2).unwrap();
        assert_eq!(related_2hops.len(), 2);
        // First hop is B (PostgreSQL DB), second hop is C (NVMe Volume)
        assert_eq!(related_2hops[0].memory.id, mem_b.id);
        assert_eq!(related_2hops[0].distance, 1);
        assert_eq!(related_2hops[1].memory.id, mem_c.id);
        assert_eq!(related_2hops[1].distance, 2);

        // 1 hop only finds B
        let related_1hop = storage.traverse_multi_hop(&mem_a.id, 1).unwrap();
        assert_eq!(related_1hop.len(), 1);
        assert_eq!(related_1hop[0].memory.id, mem_b.id);

        // Test graph snapshot
        let snapshot = storage.get_graph_snapshot(None, None).unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
        assert_eq!(snapshot.edges.len(), 2);

        // Test manual link and unlink
        storage
            .add_link(&mem_a.id, &mem_c.id, Some("depends_on"), Some(2.0))
            .unwrap();
        let links_after_manual = storage.get_links_for_memory(&mem_a.id).unwrap();
        assert_eq!(links_after_manual.len(), 2);

        storage
            .remove_link(&mem_a.id, &mem_c.id, Some("depends_on"))
            .unwrap();
        let links_after_remove = storage.get_links_for_memory(&mem_a.id).unwrap();
        assert_eq!(links_after_remove.len(), 1);

        // Test foreign key cascading on hard delete
        storage.forget_memory(&mem_b.id, true).unwrap();
        let links_after_cascade = storage.get_links_for_memory(&mem_a.id).unwrap();
        assert_eq!(links_after_cascade.len(), 0);
    }

    #[test]
    fn test_autolink_vault() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("autolink_test.db");
        let storage = Storage::open(&db_path).unwrap();

        // 1. Memory A mentions Memory B's title in plain text
        let mem_b = MemoryRecord::new(
            MemoryType::Fact,
            "Redis Cache".to_string(),
            "Redis in-memory caching cluster running on port 6379".to_string(),
            vec!["redis".to_string(), "cache".to_string()],
            0.9,
            None,
        );
        let vec_b = vec![0.5f32, 0.5, 0.5, 0.5];
        storage.insert_memory(&mem_b, Some(&vec_b)).unwrap();

        let mem_a = MemoryRecord::new(
            MemoryType::Instruction,
            "Auth Service Setup".to_string(),
            "Connect user sessions to Redis Cache before accepting API traffic".to_string(),
            vec!["auth".to_string()],
            0.9,
            None,
        );
        let vec_a = vec![0.1f32, 0.9, 0.0, 0.0];
        storage.insert_memory(&mem_a, Some(&vec_a)).unwrap();

        // 2. Memory C and Memory D share 2 non-generic tags ("database", "cluster")
        let mem_c = MemoryRecord::new(
            MemoryType::Decision,
            "Database Replication".to_string(),
            "Postgres primary replicates WAL to two standby read replicas".to_string(),
            vec!["database".to_string(), "cluster".to_string(), "postgres".to_string()],
            0.9,
            None,
        );
        let vec_c = vec![0.8f32, 0.2, 0.1, 0.0];
        storage.insert_memory(&mem_c, Some(&vec_c)).unwrap();

        let mem_d = MemoryRecord::new(
            MemoryType::Fact,
            "Failover Mechanism".to_string(),
            "Patroni triggers leader election if primary heartbeat misses 3 pings".to_string(),
            vec!["database".to_string(), "cluster".to_string(), "patroni".to_string()],
            0.9,
            None,
        );
        // Very similar vector to mem_c (cosine sim > 0.95)
        let vec_d = vec![0.82f32, 0.19, 0.08, 0.0];
        storage.insert_memory(&mem_d, Some(&vec_d)).unwrap();

        // Run autolink
        let new_links = storage.autolink_vault(0.75).unwrap();
        assert!(new_links >= 3, "Expected at least 3 links created, got {}", new_links);

        // Verify mem_a mentions mem_b
        let links_a = storage.get_links_for_memory(&mem_a.id).unwrap();
        assert!(links_a.iter().any(|l| l.target_id == mem_b.id && l.relation == "mentions"));

        // Verify mem_c and mem_d are linked via shared_topic and relates_to
        let links_c = storage.get_links_for_memory(&mem_c.id).unwrap();
        assert!(links_c.iter().any(|l| l.target_id == mem_d.id && (l.relation == "shared_topic" || l.relation == "relates_to")));

        // Idempotency: second run should add 0 new links
        let second_run = storage.autolink_vault(0.75).unwrap();
        assert_eq!(second_run, 0);
    }
}

