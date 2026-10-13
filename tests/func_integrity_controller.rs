use chrono::Utc;
use lightmem::models::{compute_sha256_hex, LightMemConfig, MemoryStatus, MemoryType};
use lightmem::LightMem;
use std::sync::Mutex;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn hash_config() -> LightMemConfig {
    LightMemConfig {
        backend: "hash".to_string(),
        reranker: "top1".to_string(),
        ..LightMemConfig::default()
    }
}

// ============================================================================
// 1. LIFECYCLE, CONFIG, EMBEDDING IDENTITY & REINDEX
// ============================================================================

#[test]
fn func_controller_open_default_open_at_and_reindex() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("ctrl.db");
    let cfg = hash_config();

    let mem = LightMem::open_at(&db_path, cfg.clone()).unwrap();
    assert_eq!(mem.db_path(), db_path.as_path());
    assert_eq!(mem.config().backend, "hash");
    assert_eq!(
        mem.requested_embedding_identity(),
        "card-v1:hash:fnv-bigram-v1"
    );
    assert_eq!(mem.stored_embedding_identity().unwrap(), None);
    assert!(!mem.embedding_migration_needed().unwrap());

    mem.remember(
        "Rust ownership guarantees memory safety without garbage collection.",
        Some(MemoryType::Fact),
        Some("Rust Ownership".to_string()),
        vec!["rust".to_string()],
        Some(0.95),
    )
    .unwrap();

    assert_eq!(
        mem.stored_embedding_identity().unwrap().as_deref(),
        Some("card-v1:hash:fnv-bigram-v1")
    );
    assert!(!mem.embedding_migration_needed().unwrap());

    let mut progress_steps = Vec::new();
    let count = mem
        .reindex(|done, total| progress_steps.push((done, total)))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(progress_steps.first(), Some(&(0, 1)));
    assert_eq!(progress_steps.last(), Some(&(1, 1)));

    // Also verify open_default with isolated environment variables
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cfg_dir = dir.path().join("cfg");
    let default_db = dir.path().join("default_env.db");
    std::env::set_var("LIGHTMEM_CONFIG_DIR", &cfg_dir);
    std::env::set_var("LIGHTMEM_DB", &default_db);
    cfg.save().unwrap();

    let default_mem = LightMem::open_default(true).unwrap();
    assert_eq!(default_mem.db_path(), default_db.as_path());

    std::env::remove_var("LIGHTMEM_CONFIG_DIR");
    std::env::remove_var("LIGHTMEM_DB");
}

#[test]
fn integrity_controller_embedding_migration_guard_and_unknown_backend() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("mig.db");

    // 1. Unknown backend is rejected at open_at
    let bad_cfg = LightMemConfig {
        backend: "nonexistent-backend".to_string(),
        ..LightMemConfig::default()
    };
    assert!(LightMem::open_at(&db_path, bad_cfg).is_err());

    // 2. Populate DB with hash backend
    let mem_hash = LightMem::open_at(&db_path, hash_config()).unwrap();
    mem_hash
        .remember(
            "Initial memory indexed with hash provider.",
            Some(MemoryType::Fact),
            None,
            vec![],
            None,
        )
        .unwrap();
    drop(mem_hash);

    // 3. Re-open with a different embedding identity (e.g. ollama model identity)
    let ollama_cfg = LightMemConfig {
        backend: "ollama".to_string(),
        ollama_url: "http://127.0.0.1:9".to_string(),
        embedding_model: "nomic-embed-text".to_string(),
        reranker: "top1".to_string(),
        ..LightMemConfig::default()
    };
    let mem_mismatch = LightMem::open_at(&db_path, ollama_cfg).unwrap();
    assert!(mem_mismatch.embedding_migration_needed().unwrap());
    assert_eq!(
        mem_mismatch.stored_embedding_identity().unwrap().as_deref(),
        Some("card-v1:hash:fnv-bigram-v1")
    );
    assert_eq!(
        mem_mismatch.requested_embedding_identity(),
        "card-v1:ollama:http://127.0.0.1:9:nomic-embed-text"
    );

    // Writes and recalls are blocked until reindex is executed
    assert!(mem_mismatch
        .remember("Should be blocked by identity mismatch", None, None, vec![], None)
        .is_err());
    assert!(mem_mismatch.recall("Initial", None, None, 5, None).is_err());
}

// ============================================================================
// 2. REMEMBER, OPTIONS, WIKILINKS & ATTACHMENTS
// ============================================================================

#[test]
fn func_controller_remember_options_ttl_wikilinks_and_attachments() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("remember.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    // Target node for wikilink auto-linking
    let target = mem
        .remember(
            "PostgreSQL 16 runs on port 5432 in production.",
            Some(MemoryType::Decision),
            Some("Postgres Production".to_string()),
            vec!["db".to_string()],
            Some(0.95),
        )
        .unwrap();

    // Remember with options including [[Postgres Production]] wikilink and TTL
    let (linked_rec, conflicts) = mem
        .remember_with_options(
            "The API service connects to [[Postgres Production]] using PgBouncer.",
            Some(MemoryType::Decision),
            Some("API Database Connection".to_string()),
            vec!["api".to_string()],
            Some(0.92),
            Some(chrono::Duration::hours(24)),
            false,
            Some("unit_test_provenance".to_string()),
        )
        .unwrap();

    assert!(conflicts.is_empty());
    assert!(linked_rec.expired_at.is_some());
    assert_eq!(linked_rec.provenance.as_str(), "unit_test_provenance");

    // Verify auto-created wikilink edge in graph
    let links = mem.get_links(&linked_rec.id).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target_id, target.id);
    assert_eq!(links[0].relation, "references");

    // Remember with file attachments (no text provided -> auto-generates title & summary)
    let img_path = dir.path().join("diagram.png");
    let png_bytes: Vec<u8> = vec![
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x08, 0x02, 0x00, 0x00, 0x00,
    ];
    std::fs::write(&img_path, &png_bytes).unwrap();

    let (att_rec, _) = mem
        .remember_with_attachments(
            None,
            &[img_path],
            false,
            None,
            None,
            None,
            vec!["arch".to_string()],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    assert_eq!(att_rec.category, MemoryType::Artifact);
    assert_eq!(att_rec.title, "diagram.png");
    assert!(att_rec.tags.contains(&"image".to_string()));
    assert_eq!(att_rec.attachments.len(), 1);
    assert_eq!(att_rec.attachments[0].dimensions.as_deref(), Some("32x16"));
}

#[test]
fn integrity_controller_remember_rejects_blank_invalid_confidence_and_deduplicates_files() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("remember_int.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    // Blank / whitespace content rejected
    assert!(mem.remember("   \n\t  ", None, None, vec![], None).is_err());
    assert!(mem
        .remember_with_attachments(
            Some("   "),
            &[],
            false,
            None,
            None,
            None,
            vec![],
            None,
            None,
            false,
            None
        )
        .is_err());

    // Invalid confidence rejected
    assert!(mem
        .remember("Valid text", None, None, vec![], Some(-0.1))
        .is_err());
    assert!(mem
        .remember("Valid text", None, None, vec![], Some(1.5))
        .is_err());
    assert!(mem
        .remember("Valid text", None, None, vec![], Some(f32::NAN))
        .is_err());

    // Duplicate attachment paths in the same call are deduplicated by SHA-256
    let file_a = dir.path().join("dup_a.txt");
    let file_b = dir.path().join("dup_b.txt");
    std::fs::write(&file_a, b"identical payload bytes").unwrap();
    std::fs::write(&file_b, b"identical payload bytes").unwrap();

    let (rec, _) = mem
        .remember_with_attachments(
            Some("Memory with duplicate files passed"),
            &[file_a, file_b],
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    assert_eq!(rec.attachments.len(), 1);
}

// ============================================================================
// 3. CONFLICTS & MERGE_CONFLICT_PAIR
// ============================================================================

#[test]
fn func_controller_find_conflicts_and_merge_conflict_pair() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("conflicts.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    let older = mem
        .remember(
            "User prefers dark mode theme in VS Code editor.",
            Some(MemoryType::Preference),
            Some("Editor Theme Preference".to_string()),
            vec!["vscode".to_string(), "theme".to_string()],
            Some(0.85),
        )
        .unwrap();

    let newer = mem
        .remember(
            "User prefers light mode theme in VS Code editor.",
            Some(MemoryType::Preference),
            Some("Editor Theme Preference".to_string()),
            vec!["vscode".to_string(), "theme".to_string()],
            Some(0.95),
        )
        .unwrap();

    let conflicts = mem
        .find_conflicts_with_options(false, 0.50, Some("top1"))
        .unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].older_memory.id, older.id);
    assert_eq!(conflicts[0].newer_memory.id, newer.id);

    // Also test default find_conflicts(false)
    let _ = mem.find_conflicts(false).unwrap();
}

#[test]
fn integrity_controller_conflict_merge_transfers_attachments_links_and_retires_older() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("conflict_merge.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    let anchor = mem
        .remember(
            "Anchor workspace configuration node.",
            Some(MemoryType::Context),
            Some("Workspace Anchor".to_string()),
            vec![],
            None,
        )
        .unwrap();

    let att_file = dir.path().join("theme_spec.txt");
    std::fs::write(&att_file, b"theme-spec-v1").unwrap();

    let (older, _) = mem
        .remember_with_attachments(
            Some("Deployment port for backend service is 8080 in staging."),
            &[att_file],
            false,
            None,
            Some(MemoryType::Fact),
            Some("Backend Service Port".to_string()),
            vec!["staging".to_string()],
            Some(0.85),
            None,
            false,
            None,
        )
        .unwrap();

    mem.link(&older.id, &anchor.id, Some("depends_on"), Some(0.9))
        .unwrap();

    let newer = mem
        .remember(
            "Deployment port for backend service is 9090 in staging.",
            Some(MemoryType::Fact),
            Some("Backend Service Port".to_string()),
            vec!["staging".to_string()],
            Some(0.95),
        )
        .unwrap();

    let conflicts = mem
        .find_conflicts_with_options(false, 0.50, Some("top1"))
        .unwrap();
    assert_eq!(conflicts.len(), 1);

    let merged = mem
        .merge_conflict_pair(&conflicts[0], Some("top1"))
        .unwrap();
    assert_eq!(merged.id, newer.id);

    // Older memory must now be Expired
    let older_reloaded = mem.get(&older.id).unwrap().unwrap();
    assert_eq!(older_reloaded.status, MemoryStatus::Expired);

    // Survivor must have inherited the attachment and the graph link from older
    let survivor_atts = mem.get_attachments_data(&merged.id).unwrap();
    assert_eq!(survivor_atts.len(), 1);
    assert_eq!(survivor_atts[0].1, b"theme-spec-v1");

    let survivor_links = mem.get_links(&merged.id).unwrap();
    assert!(survivor_links.iter().any(|l| l.target_id == anchor.id));
}

// ============================================================================
// 4. RECALL, MULTI-HOP EXPANSION, LIST, PAGINATION & GET
// ============================================================================

#[test]
fn func_controller_recall_expanded_list_pagination_and_get() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("recall_list.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    let m1 = mem
        .remember(
            "Primary Redis cluster caches session tokens.",
            Some(MemoryType::Decision),
            Some("Redis Session Cache".to_string()),
            vec!["redis".to_string()],
            Some(0.95),
        )
        .unwrap();

    let m2 = mem
        .remember(
            "Sentinel failover monitors replica health every 5 seconds.",
            Some(MemoryType::Decision),
            Some("Sentinel Failover".to_string()),
            vec!["ops".to_string()],
            Some(0.90),
        )
        .unwrap();

    mem.link(&m1.id, &m2.id, Some("relates_to"), Some(1.0))
        .unwrap();

    // Standard recall & recall_with_date
    let today = Utc::now().date_naive();
    let base_hits = mem
        .recall_with_date("Redis session tokens", None, None, Some(today), 5, None)
        .unwrap();
    assert!(!base_hits.is_empty());
    assert_eq!(base_hits[0].memory.id, m1.id);

    // Multi-hop expanded recall pulls in linked neighbor m2
    let expanded_hits = mem
        .recall_expanded(
            "Redis session tokens",
            None,
            None,
            Some(today),
            1,
            None,
            true,
        )
        .unwrap();
    assert!(expanded_hits.iter().any(|h| h.memory.id == m1.id));
    assert!(expanded_hits.iter().any(|h| h.memory.id == m2.id));

    // Count, list, list_paginated, list_page, and get
    assert_eq!(mem.count(None, Some(MemoryStatus::Active), None).unwrap(), 2);
    assert_eq!(mem.list(None, None, None, 10).unwrap().len(), 2);

    let p1 = mem
        .list_paginated_with_date(None, None, None, Some(today), 1, 0)
        .unwrap();
    assert_eq!(p1.total, 2);
    assert_eq!(p1.items.len(), 1);
    assert!(p1.has_more);

    let page2 = mem
        .list_page_with_date(None, None, None, Some(today), 2, 1)
        .unwrap();
    assert_eq!(page2.page, 2);
    assert_eq!(page2.items.len(), 1);
    assert!(!page2.has_more);

    // Get by title and by ID prefix
    let by_title = mem.get("Redis Session Cache").unwrap().unwrap();
    assert_eq!(by_title.id, m1.id);
    let by_prefix = mem.get(&m1.id[..8]).unwrap().unwrap();
    assert_eq!(by_prefix.id, m1.id);
}

#[test]
fn integrity_controller_pagination_bounds_and_recall_validation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("bounds.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    // Page 0 or per_page 0 must error
    assert!(mem.list_page(None, None, None, 0, 10).is_err());
    assert!(mem.list_page(None, None, None, 1, 0).is_err());
    // Overflowing offset must error cleanly without panic
    assert!(mem.list_page(None, None, None, usize::MAX, 2).is_err());

    // Blank recall query or invalid min_similarity must error
    assert!(mem.recall("   ", None, None, 5, None).is_err());
    assert!(mem.recall("valid", None, None, 5, Some(1.5)).is_err());
}

// ============================================================================
// 5. ATTACHMENT MATERIALIZATION, DEDUPLICATE, FORGET & CLEAR_ALL
// ============================================================================

#[test]
fn func_controller_attachments_materialize_deduplicate_forget_and_clear() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("att_lifecycle.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    let src_file = dir.path().join("notes.txt");
    std::fs::write(&src_file, b"attachment-payload-123").unwrap();

    let (m1, _) = mem
        .remember_with_attachments(
            Some("First note with file"),
            &[src_file],
            false,
            None,
            Some(MemoryType::Fact),
            Some("Note One".to_string()),
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    // Materialize to default_attachments_dir
    let mat_paths = mem.materialize_attachments(&m1.id, None).unwrap();
    assert_eq!(mat_paths.len(), 1);
    assert!(mat_paths[0].exists());
    assert!(mat_paths[0].starts_with(
        std::fs::canonicalize(mem.default_attachments_dir())
            .unwrap_or_else(|_| mem.default_attachments_dir())
    ));

    // Export to custom directory via save_attachments_to & save_memories_attachments_to
    let custom_dir = dir.path().join("exported_atts");
    let saved = mem.save_attachments_to(&m1.id, &custom_dir).unwrap();
    assert_eq!(saved.len(), 1);
    let saved_multi = mem
        .save_memories_attachments_to(&[m1.id.clone()], &custom_dir.join("multi"))
        .unwrap();
    assert_eq!(saved_multi.len(), 1);

    // Soft forget marks status Expired
    assert!(mem.forget(&m1.id, false).unwrap());
    assert_eq!(
        mem.get(&m1.id).unwrap().unwrap().status,
        MemoryStatus::Expired
    );
}

#[test]
fn integrity_controller_hard_forget_and_clear_cleanup_unreferenced_materialized_files() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("att_cleanup.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    let shared_file = dir.path().join("shared.bin");
    let payload = b"shared-immutable-binary-blob";
    let expected_sha = compute_sha256_hex(payload);
    std::fs::write(&shared_file, payload).unwrap();

    let (m1, _) = mem
        .remember_with_attachments(
            Some("First holder of shared blob"),
            std::slice::from_ref(&shared_file),
            false,
            None,
            Some(MemoryType::Artifact),
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    let (m2, _) = mem
        .remember_with_attachments(
            Some("Second holder of shared blob"),
            std::slice::from_ref(&shared_file),
            false,
            None,
            Some(MemoryType::Artifact),
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    assert_eq!(m1.attachments[0].sha256, expected_sha);
    let mat = mem.materialize_attachments(&m1.id, None).unwrap();
    assert_eq!(mat.len(), 1);
    assert!(mat[0].exists());

    // Hard-deleting m1 MUST NOT delete the materialized file because m2 still references the same SHA-256!
    assert!(mem.forget(&m1.id, true).unwrap());
    assert!(
        mat[0].exists(),
        "Materialized file must remain while m2 still references the SHA-256"
    );

    // Hard-deleting m2 now leaves 0 references -> materialized file must be cleaned up!
    assert!(mem.forget(&m2.id, true).unwrap());
    assert!(
        !mat[0].exists(),
        "Materialized file must be deleted once no memory references its SHA-256"
    );

    // Verify clear_all(true) removes the entire default_attachments_dir
    let (m3, _) = mem
        .remember_with_attachments(
            Some("Third holder"),
            &[shared_file],
            false,
            None,
            Some(MemoryType::Artifact),
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();
    let _ = mem.materialize_attachments(&m3.id, None).unwrap();
    assert!(mem.default_attachments_dir().exists());

    let removed = mem.clear_all(true).unwrap();
    assert_eq!(removed, 1);
    assert!(!mem.default_attachments_dir().exists());
}

// ============================================================================
// 6. GRAPH, EXPORT/IMPORT, RERANKER/ANSWER & OBSERVER
// ============================================================================

#[test]
fn func_controller_graph_export_import_answer_and_observe() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("full_flow.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    // 1. Observe & batch save candidates
    let transcript = "User: I always use 4 spaces for Rust indentation.\nAssistant: Got it.\nDecision: We chose SQLite WAL mode for concurrent reads.";
    let candidates = mem
        .observe_and_extract(transcript, None, Some(&["obs".to_string()]), Some(0.5), None)
        .unwrap();
    assert!(candidates.len() >= 2);

    let first_saved = mem.save_observed_candidate(&candidates[0]).unwrap();
    let rest_saved = mem.save_observed_candidates(&candidates[1..]).unwrap();
    assert!(!first_saved.id.is_empty());
    assert!(!rest_saved.is_empty());

    // 2. Graph operations: link, get_links, related, graph, autolink, unlink
    let second_id = &rest_saved[0].id;
    mem.link(&first_saved.id, second_id, Some("relates_to"), Some(0.8))
        .unwrap();
    assert_eq!(mem.get_links(&first_saved.id).unwrap().len(), 1);
    assert_eq!(mem.related(&first_saved.id, 2).unwrap().len(), 1);
    let snap = mem.graph(Some(&first_saved.id), Some(1)).unwrap();
    assert_eq!(snap.nodes.len(), 2);
    let _ = mem.autolink(Some(0.75)).unwrap();
    assert!(mem
        .unlink(&first_saved.id, second_id, Some("relates_to"))
        .unwrap());

    // 3. Answer & Answer with reranker / options
    let ans = mem
        .answer("What mode did we choose for SQLite?", None, None, 5, false)
        .unwrap();
    assert!(ans.selected_memory.is_some());
    assert!(ans.answer.contains("SQLite WAL"));

    let ans_rerank = mem
        .answer_with_reranker(
            "What mode did we choose for SQLite?",
            None,
            None,
            5,
            Some("top1"),
        )
        .unwrap();
    assert!(ans_rerank.selected_memory.is_some());

    // 4. Stats, Export JSON & OKF, Import into fresh controller
    let st = mem.stats().unwrap();
    assert!(st.total_memories >= 2);

    let json_export = dir.path().join("backup.json");
    let okf_export = dir.path().join("okf_bundle");
    mem.export_json(&json_export).unwrap();
    mem.export_okf(Some(&okf_export)).unwrap();

    let db2_path = dir.path().join("restored.db");
    let mem2 = LightMem::open_at(&db2_path, hash_config()).unwrap();
    let imported_count = mem2.import_file(&okf_export).unwrap();
    assert_eq!(imported_count, st.total_memories);
    assert_eq!(mem2. deduplicate().unwrap(), 0);
}

#[test]
fn integrity_controller_atomic_directory_import_rollback_and_unknown_reranker() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("import_rollback.db");
    let mem = LightMem::open_at(&db_path, hash_config()).unwrap();

    // Unknown reranker must fail deterministically
    assert!(mem.build_reranker(Some("unknown-reranker-xyz")).is_err());

    // Create a directory with 1 valid JSON file and 1 corrupt JSON file
    let import_dir = dir.path().join("batch_import");
    std::fs::create_dir_all(&import_dir).unwrap();
    std::fs::write(
        import_dir.join("01_valid.json"),
        r#"[{"content": "This valid record must not be committed if file 02 fails.", "confidence": 0.9}]"#,
    )
    .unwrap();
    std::fs::write(
        import_dir.join("02_corrupt.json"),
        r#"[{"content": "Out of bounds confidence", "confidence": 9.99}]"#,
    )
    .unwrap();

    // Directory import must fail AND roll back completely so 0 records are stored
    assert!(mem.import_file(&import_dir).is_err());
    assert_eq!(mem.count(None, None, None).unwrap(), 0);
}
