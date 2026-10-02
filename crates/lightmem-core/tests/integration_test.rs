use chrono::{Duration, Utc};
use lightmem_core::{LightMem, LightMemConfig, MemoryStatus, MemoryType};
use tempfile::tempdir;

#[test]
fn test_core_lifecycle() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let config = LightMemConfig {
        backend: "hash".to_string(), // fast offline test
        ..Default::default()
    };

    let lm = LightMem::open_at(&db_path, config).expect("Failed to open LightMem");

    // 1. Remember
    let mem1 = lm
        .remember(
            "PostgreSQL 16 runs on port 5432 with replication enabled",
            Some(MemoryType::Fact),
            Some("Postgres DB".to_string()),
            vec!["database".to_string(), "postgres".to_string()],
            Some(0.95),
        )
        .expect("remember failed");

    let mem2 = lm
        .remember(
            "Redis cluster runs on port 6379 for caching sessions",
            Some(MemoryType::Fact),
            Some("Redis Cache".to_string()),
            vec!["cache".to_string(), "redis".to_string()],
            Some(0.90),
        )
        .expect("remember failed");

    // 2. Recall
    let recalled = lm
        .recall("postgres port", None, None, 5, None)
        .expect("recall failed");
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].memory.id, mem1.id);

    // 3. Short prefix retrieval
    let short_id: String = mem1.id.chars().take(8).collect();
    let fetched = lm.get(&short_id).expect("get failed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, mem1.id);

    // 4. Answer (Top-1 fallback)
    let ans = lm
        .answer("what port does redis use?", None, None, 5, false)
        .expect("answer failed");
    assert_eq!(ans.reranker_used, "top1");
    assert!(ans.selected_memory.is_some());

    // 5. Stats
    let stats = lm.stats().expect("stats failed");
    assert_eq!(stats.total_memories, 2);
    assert_eq!(stats.active_memories, 2);

    // 6. Forget (soft) with short prefix
    let short_id2: String = mem2.id.chars().take(8).collect();
    let forgot = lm.forget(&short_id2, false).expect("forget failed");
    assert!(forgot);

    // After soft-forget, active count should be 1
    let stats_after = lm.stats().expect("stats failed");
    assert_eq!(stats_after.active_memories, 1);
    assert_eq!(stats_after.expired_memories, 1);

    // 7. Temporal query: before expiration, mem2 should be visible
    let as_of_now = Utc::now() + Duration::seconds(10);
    let active_now = lm
        .list(None, Some(MemoryStatus::Active), Some(as_of_now), 10)
        .expect("list failed");
    assert_eq!(active_now.len(), 1); // only mem1 active now

    // 8. OKF Export & Import
    let okf_path = dir.path().join("memories.okf");
    let exported = lm.export_okf(Some(&okf_path)).expect("export failed");
    assert!(exported.exists());

    let imported_count = lm.import_file(&exported).expect("import failed");
    assert!(imported_count >= 1);
}
