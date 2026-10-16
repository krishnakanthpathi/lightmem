use chrono::{Duration, Utc};
use lightmem::models::MemoryLink;
use lightmem::repositories::sqlite_repo::{duplicate_key, duplicate_key_for_record};
use lightmem::{
    AttachmentInput, EmbeddingProvider, HashEmbeddingProvider, MemoryRecord, MemoryStatus,
    MemoryType, Storage,
};
use rusqlite::Connection;
use tempfile::tempdir;

struct FixedEmbedder(Vec<f32>);
impl EmbeddingProvider for FixedEmbedder {
    fn name(&self) -> &str {
        "fixed"
    }
    fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.0.clone())
    }
}

struct FailingEmbedder;
impl EmbeddingProvider for FailingEmbedder {
    fn name(&self) -> &str {
        "failing"
    }
    fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("simulated embedder failure")
    }
}

struct WrongBatchSizeEmbedder;
impl EmbeddingProvider for WrongBatchSizeEmbedder {
    fn name(&self) -> &str {
        "wrong-batch"
    }
    fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![]) // Mismatched batch count!
    }
}

fn make_mem(id: &str, title: &str, content: &str, cat: MemoryType) -> MemoryRecord {
    let mut m = MemoryRecord::new(
        cat,
        title.to_string(),
        content.to_string(),
        vec![],
        0.9,
        None,
    );
    m.id = id.to_string();
    m
}

// ============================================================================
// 1. Storage Lifecycle, Open, Migrate & Permissions
// ============================================================================

#[test]
fn func_storage_open_and_in_memory() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("nested/dir/vault.db");
    let storage = Storage::open(&db_path).unwrap();
    assert_eq!(storage.path(), db_path.as_path());
    assert!(db_path.exists());

    let in_mem = Storage::open_in_memory().unwrap();
    assert_eq!(in_mem.stats().unwrap().total_memories, 0);
}

#[test]
fn integrity_storage_open_permissions_future_version_and_legacy_repair() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("vault.db");
    let storage = Storage::open(&db_path).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Database file must be owner-only (0600)");
    }

    let m = make_mem(
        "m1",
        "Legacy Tag Row",
        "Content for repair",
        MemoryType::Fact,
    );
    storage.insert_memory(&m, None).unwrap();
    drop(storage);

    // Corrupt row to legacy comma-separated tags and blank dedup_key at user_version = 4
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE memories SET tags = 'alpha, beta, gamma', dedup_key = '' WHERE id = 'm1'",
            [],
        )
        .unwrap();
    }

    let reopened = Storage::open(&db_path).unwrap();
    let repaired = reopened.get_memory("m1").unwrap().unwrap();
    assert_eq!(repaired.tags, vec!["alpha", "beta", "gamma"]);
    drop(reopened);

    // Bump user_version to 99 (newer than supported v4) -> must refuse to open
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    assert!(
        Storage::open(&db_path).is_err(),
        "Must reject database schema newer than v4"
    );
}

// ============================================================================
// 2. Insert, Indexed Batch, Identity & Optimistic Reindex
// ============================================================================

#[test]
fn func_insert_memory_and_indexed_batches() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem(
        "id-11111111",
        "Redis",
        "Redis runs on port 6379",
        MemoryType::Fact,
    );
    storage
        .insert_indexed_batch(&[m1], &FixedEmbedder(vec![1.0, 0.0]), "id-v1", true)
        .unwrap();

    assert_eq!(
        storage.embedding_identity().unwrap().as_deref(),
        Some("id-v1")
    );
    assert!(storage.check_embedding_identity("id-v1").is_ok());

    let att = AttachmentInput::from_bytes("note.txt", b"hello attachment".to_vec());
    let m2 = make_mem("id-22222222", "Note", "Attached note", MemoryType::Artifact);
    let stored = storage
        .insert_indexed_with_attachments(&m2, &[att], &FixedEmbedder(vec![0.0, 1.0]), "id-v1", true)
        .unwrap();
    assert_eq!(stored.attachments.len(), 1);
}

#[test]
fn integrity_insert_memory_validation_and_batch_rollback() {
    let storage = Storage::open_in_memory().unwrap();

    // Blank content or ID must be rejected
    let blank = make_mem("id-blank", "Title", "   ", MemoryType::Fact);
    assert!(storage.insert_memory(&blank, None).is_err());

    let mut bad_conf = make_mem("id-nan", "Title", "Valid content", MemoryType::Fact);
    bad_conf.confidence = f32::NAN;
    assert!(storage.insert_memory(&bad_conf, None).is_err());

    // Failing embedder or wrong batch size must roll back the entire transaction
    let valid = make_mem("id-valid", "Valid", "Valid content", MemoryType::Fact);
    assert!(storage
        .insert_indexed_batch(
            std::slice::from_ref(&valid),
            &FailingEmbedder,
            "id-v1",
            true
        )
        .is_err());
    assert!(storage
        .insert_indexed_batch(&[valid], &WrongBatchSizeEmbedder, "id-v1", true)
        .is_err());
    assert_eq!(storage.stats().unwrap().total_memories, 0);
}

#[test]
fn func_index_snapshot_and_replace_index() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem("m1", "One", "First record", MemoryType::Fact);
    storage
        .insert_indexed_batch(&[m1], &FixedEmbedder(vec![1.0, 0.0]), "old-id", true)
        .unwrap();

    let (rev, records) = storage.index_snapshot().unwrap();
    assert_eq!(records.len(), 1);

    storage
        .replace_index(rev, "new-id", &[("m1".to_string(), vec![0.0, 1.0, 0.0])])
        .unwrap();
    assert_eq!(
        storage.embedding_identity().unwrap().as_deref(),
        Some("new-id")
    );
    let vecs = storage
        .get_candidate_vectors_checked(None, None, None, Some("new-id"))
        .unwrap();
    assert_eq!(vecs[0].1, vec![0.0, 1.0, 0.0]);
}

#[test]
fn integrity_replace_index_rejects_stale_revision_missing_ids_and_nan() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem("m1", "One", "First record", MemoryType::Fact);
    storage
        .insert_indexed_batch(&[m1], &FixedEmbedder(vec![1.0, 0.0]), "old-id", true)
        .unwrap();

    let (rev, _) = storage.index_snapshot().unwrap();
    // Stale revision
    assert!(storage
        .replace_index(rev + 99, "new-id", &[("m1".to_string(), vec![1.0, 0.0])])
        .is_err());
    // Missing IDs
    assert!(storage.replace_index(rev, "new-id", &[]).is_err());
    // NaN vector
    assert!(storage
        .replace_index(rev, "new-id", &[("m1".to_string(), vec![f32::NAN, 0.0])])
        .is_err());
    // Old index remains intact
    assert_eq!(
        storage.embedding_identity().unwrap().as_deref(),
        Some("old-id")
    );
}

// ============================================================================
// 3. Deduplication & Duplicate Keys
// ============================================================================

#[test]
fn func_deduplicate_and_merge_and_duplicate_keys() {
    assert_eq!(
        duplicate_key("  preserve Case & spaces  "),
        "preserve Case & spaces"
    );

    let mut rec = make_mem("m1", "T", "Body", MemoryType::Fact);
    let att = AttachmentInput::from_bytes("a.txt", b"payload".to_vec());
    rec.attachments.push(att.metadata);
    assert!(duplicate_key_for_record(&rec).contains("\0att:"));

    let storage = Storage::open_in_memory().unwrap();
    // Insert two identical records with merge=false via raw SQL or insert_indexed_batch(merge=false)
    let mut r1 = make_mem("dup-1", "Auto Title", "Same content", MemoryType::Fact);
    r1.tags = vec!["tag1".to_string()];
    r1.confidence = 0.7;
    let mut r2 = make_mem("dup-2", "Curated Title", "Same content", MemoryType::Fact);
    r2.tags = vec!["tag2".to_string()];
    r2.confidence = 0.95;

    let emb = HashEmbeddingProvider;
    storage
        .insert_indexed_batch(&[r1], &emb, "hash-id", false)
        .unwrap();
    storage
        .insert_indexed_batch(&[r2], &emb, "hash-id", false)
        .unwrap();
    assert_eq!(storage.count_memories(None, None, None).unwrap(), 2);

    let removed = storage.deduplicate_indexed(&emb, "hash-id").unwrap();
    assert_eq!(removed, 1);
    assert_eq!(storage.count_memories(None, None, None).unwrap(), 1);
    let survivor = storage
        .list_memories(None, None, None, 10)
        .unwrap()
        .remove(0);
    assert_eq!(survivor.confidence, 0.95);
    assert_eq!(survivor.tags.len(), 2);
}

#[test]
fn integrity_deduplicate_unindexed_invalidates_embedding_state() {
    let storage = Storage::open_in_memory().unwrap();
    let r1 = make_mem("d1", "T1", "Dup text", MemoryType::Fact);
    let r2 = make_mem("d2", "T2", "Dup text", MemoryType::Fact);
    let emb = HashEmbeddingProvider;
    storage
        .insert_indexed_batch(&[r1], &emb, "hash-id", false)
        .unwrap();
    storage
        .insert_indexed_batch(&[r2], &emb, "hash-id", false)
        .unwrap();

    let removed = storage.deduplicate_and_merge().unwrap();
    assert_eq!(removed, 1);
    assert_eq!(
        storage.embedding_identity().unwrap(),
        None,
        "Unindexed deduplication must invalidate embedding_state"
    );
}

// ============================================================================
// 4. Retrieval, Prefix/Title Resolution & Expiration
// ============================================================================

#[test]
fn func_get_memory_exact_prefix_and_title_and_expire_due() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem(
        "aaaa1111-2222",
        "Kubernetes Ingress Controller",
        "Nginx ingress runs on 443",
        MemoryType::Fact,
    );
    let mut m_due = make_mem(
        "bbbb1111-2222",
        "Ephemeral Token",
        "Temporary token",
        MemoryType::Password,
    );
    m_due.expired_at = Some(Utc::now() - Duration::seconds(5));

    storage.insert_memory(&m1, None).unwrap();
    storage.insert_memory(&m_due, None).unwrap();

    // Expire due memories
    let expired_count = storage.expire_due_memories().unwrap();
    assert_eq!(expired_count, 1);
    assert_eq!(
        storage.get_memory("bbbb1111").unwrap().unwrap().status,
        MemoryStatus::Expired
    );

    // Prefix lookup
    assert_eq!(
        storage.get_memory("aaaa11").unwrap().unwrap().id,
        "aaaa1111-2222"
    );
    // Case-insensitive exact title lookup
    assert_eq!(
        storage
            .get_memory_by_id_or_title("kubernetes ingress controller")
            .unwrap()
            .unwrap()
            .id,
        "aaaa1111-2222"
    );
    // Unique title substring lookup
    assert_eq!(
        storage
            .get_memory_by_id_or_title("Ingress")
            .unwrap()
            .unwrap()
            .id,
        "aaaa1111-2222"
    );
    // Batch exact lookup
    let exact_map = storage
        .get_memories_exact(&["aaaa1111-2222".to_string(), "missing".to_string()])
        .unwrap();
    assert_eq!(exact_map.len(), 1);
}

#[test]
fn integrity_get_memory_rejects_empty_and_ambiguous_prefix() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem("abcd-1111", "First", "First content", MemoryType::Fact);
    let m2 = make_mem("abcd-2222", "Second", "Second content", MemoryType::Fact);
    storage.insert_memory(&m1, None).unwrap();
    storage.insert_memory(&m2, None).unwrap();

    assert!(storage.get_memory("").is_err());
    let err = storage.get_memory("abcd").unwrap_err();
    assert!(err.to_string().contains("Ambiguous memory ID prefix"));
}

// ============================================================================
// 5. Attachments, Immutability Trigger & SHA-256 Verification
// ============================================================================

#[test]
fn func_attachments_save_copy_and_sha_reference() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("att.db");
    let storage = Storage::open(&db_path).unwrap();
    let emb = HashEmbeddingProvider;

    let att1 = AttachmentInput::from_bytes("report.txt", b"quarterly report".to_vec());
    let sha = att1.metadata.sha256.clone();
    let m1 = make_mem("m1", "Report", "Q1 report", MemoryType::Artifact);
    let m2 = make_mem("m2", "Archive", "Q1 archive", MemoryType::Artifact);

    storage
        .insert_indexed_with_attachments(&m1, &[att1], &emb, "hash-id", true)
        .unwrap();
    storage
        .insert_indexed_batch(&[m2], &emb, "hash-id", true)
        .unwrap();

    assert!(storage.is_attachment_sha_referenced(&sha).unwrap());

    // Copy attachment from m1 to m2
    let copied = storage
        .copy_attachments_between_memories("m1", "m2")
        .unwrap();
    assert_eq!(copied, 1);
    assert_eq!(storage.get_memory_attachments_data("m2").unwrap().len(), 1);

    // Save attachments across multiple memories with filename collision disambiguation
    let out_dir = dir.path().join("exported_atts");
    let paths = storage
        .save_memories_attachments_to(&["m1".to_string(), "m2".to_string()], &out_dir)
        .unwrap();
    assert_eq!(paths.len(), 2);
    assert_ne!(
        paths[0], paths[1],
        "Duplicate filenames across memories must be disambiguated"
    );
}

#[test]
fn integrity_attachment_sha256_corruption_detection() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tamper.db");
    let storage = Storage::open(&db_path).unwrap();
    let emb = HashEmbeddingProvider;

    let mut att = AttachmentInput::from_bytes("secret.bin", b"authentic-bytes".to_vec());
    // Intentionally mismatch the stored sha256 metadata vs raw data bytes
    att.metadata.sha256 =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    let m1 = make_mem(
        "m-tamper",
        "Tampered",
        "Tampered attachment",
        MemoryType::Artifact,
    );
    storage
        .insert_indexed_with_attachments(&m1, &[att], &emb, "hash-id", false)
        .unwrap();

    let err = storage.get_memory_attachments_data("m-tamper").unwrap_err();
    assert!(
        err.to_string()
            .contains("Attachment integrity verification failed"),
        "Must detect SHA-256 mismatch on read"
    );
}

// ============================================================================
// 6. Knowledge Graph Links, Multi-Hop Traversal & Autolink
// ============================================================================

#[test]
fn func_graph_links_restore_copy_multihop_and_autolink() {
    let storage = Storage::open_in_memory().unwrap();
    let emb = HashEmbeddingProvider;

    let m1 = make_mem(
        "node-1",
        "API Gateway",
        "Routes traffic to [[Auth Service]] and PostgresCluster",
        MemoryType::Artifact,
    );
    let m2 = make_mem(
        "node-2",
        "Auth Service",
        "Validates JWT tokens against PostgresCluster",
        MemoryType::Artifact,
    );
    let mut m3 = make_mem(
        "node-3",
        "PostgresCluster",
        "Primary relational store",
        MemoryType::Artifact,
    );
    m3.tags = vec!["database".to_string(), "production".to_string()];
    let mut m4 = make_mem(
        "node-4",
        "RedisCluster",
        "Cache store",
        MemoryType::Artifact,
    );
    m4.tags = vec!["database".to_string(), "production".to_string()];

    storage
        .insert_indexed_batch(&[m1, m2, m3, m4], &emb, "hash-id", true)
        .unwrap();

    // Auto-link wikilinks
    let auto_wikilinked = storage
        .auto_link_memory("node-1", "Routes traffic to [[Auth Service]]")
        .unwrap();
    assert_eq!(auto_wikilinked, 1);

    // Explicit link node-2 -> node-3
    storage
        .add_link("node-2", "node-3", Some("depends_on"), Some(2.0))
        .unwrap();

    // Multi-hop traversal from node-1 (2 hops: node-1 -> node-2 -> node-3)
    let related = storage.traverse_multi_hop("node-1", 2).unwrap();
    assert_eq!(related.len(), 2);
    assert!(related
        .iter()
        .any(|r| r.memory.id == "node-3" && r.distance == 2));

    // Autolink vault (discovers title mentions & >=2 shared non-generic tags)
    let created_links = storage.autolink_vault(0.99).unwrap();
    assert!(created_links >= 2);

    // Graph snapshot (full and focused)
    let full_graph = storage.get_graph_snapshot(None, None).unwrap();
    assert_eq!(full_graph.nodes.len(), 4);
    let focused = storage.get_graph_snapshot(Some("node-1"), Some(1)).unwrap();
    assert!(focused.nodes.len() >= 2);

    // Copy links & remove link
    let all = storage.all_links().unwrap();
    assert!(!all.is_empty());
    assert!(storage
        .remove_link("node-2", "node-3", Some("depends_on"))
        .unwrap());
}

#[test]
fn integrity_graph_rejects_self_links_clamps_weights_and_handles_cycles() {
    let storage = Storage::open_in_memory().unwrap();
    let m1 = make_mem("n1", "Node One", "Content 1", MemoryType::Fact);
    let m2 = make_mem("n2", "Node Two", "Content 2", MemoryType::Fact);
    storage.insert_memory(&m1, None).unwrap();
    storage.insert_memory(&m2, None).unwrap();

    // Self-link must be rejected
    assert!(storage.add_link("n1", "n1", None, None).is_err());

    // Weight clamping [0.01, 10.0] and cycle creation n1 <-> n2
    storage
        .add_link("n1", "n2", Some("cycle"), Some(999.0))
        .unwrap();
    storage
        .add_link("n2", "n1", Some("cycle"), Some(-5.0))
        .unwrap();

    let links = storage.get_links_for_memory("n1").unwrap();
    assert!(links.iter().all(|l| (0.01..=10.0).contains(&l.weight)));

    // Restore links skips self-links and non-existent nodes
    let restored = storage
        .restore_links(&[
            MemoryLink::new("n1", "n1", None, None),
            MemoryLink::new("n1", "nonexistent", None, None),
        ])
        .unwrap();
    assert_eq!(restored, 0);

    // Cycle in multi-hop traversal must terminate cleanly without infinite loop
    let related = storage.traverse_multi_hop("n1", 10).unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].memory.id, "n2");
}

// ============================================================================
// 7. Pagination, Search BM25, Candidate Vectors, Forget, Clear & Stats
// ============================================================================

#[test]
fn func_list_paginated_bm25_forget_clear_and_stats() {
    let storage = Storage::open_in_memory().unwrap();
    for i in 1..=5 {
        let m = make_mem(
            &format!("id-{}", i),
            &format!("Service {}", i),
            &format!("Microservice {} runs on port 800{}", i, i),
            MemoryType::Fact,
        );
        storage.insert_memory(&m, Some(&[1.0, 0.0])).unwrap();
    }

    let page1 = storage
        .list_memories_paginated(
            Some(MemoryType::Fact),
            Some(MemoryStatus::Active),
            None,
            2,
            0,
        )
        .unwrap();
    assert_eq!(page1.total, 5);
    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.page, 1);
    assert_eq!(page1.total_pages, 3);
    assert!(page1.has_more);

    let hits = storage
        .search_bm25("port 8003", None, Some(MemoryStatus::Active), None, 5)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "id-3");

    // Soft forget id-1, hard forget id-2
    assert!(storage.forget_memory("id-1", false).unwrap());
    assert!(storage.forget_memory("id-2", true).unwrap());

    let stats = storage.stats().unwrap();
    assert_eq!(stats.total_memories, 4);
    assert_eq!(stats.active_memories, 3);
    assert_eq!(stats.expired_memories, 1);

    // Soft clear remaining active
    let soft_cleared = storage.clear_all(false).unwrap();
    assert_eq!(soft_cleared, 3);
    assert_eq!(storage.stats().unwrap().active_memories, 0);

    // Hard clear all
    let hard_cleared = storage.clear_all(true).unwrap();
    assert_eq!(hard_cleared, 4);
    assert_eq!(storage.stats().unwrap().total_memories, 0);
}

#[test]
fn integrity_bm25_injection_and_corrupt_vector_blob_rejection() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("corrupt_vec.db");
    let storage = Storage::open(&db_path).unwrap();
    let m = make_mem(
        "m1",
        "SQL Test",
        "Safe FTS5 query handling",
        MemoryType::Fact,
    );
    storage.insert_memory(&m, Some(&[1.0, 2.0])).unwrap();

    // Hostile FTS5 syntax characters must not cause SQL syntax errors
    let hostile = storage
        .search_bm25(
            "NEAR/2 \"unclosed 'quote (OR AND NOT * ^ : -",
            None,
            None,
            None,
            5,
        )
        .unwrap();
    assert!(hostile.is_empty());

    // Punctuation-only query must return empty without hitting SQLite error
    assert!(storage
        .search_bm25("??? !!! ...", None, None, None, 5)
        .unwrap()
        .is_empty());

    // Corrupt vector blob length in SQLite (dims=2 but 3 bytes blob) must be caught
    drop(storage);
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE memory_vectors SET embedding = X'010203', dims = 2 WHERE id = 'm1'",
            [],
        )
        .unwrap();
    }
    let reopened = Storage::open(&db_path).unwrap();
    assert!(
        reopened.get_candidate_vectors(None, None, None).is_err(),
        "Corrupt vector blob must return an error instead of misaligning f32 reads"
    );
}
