use chrono::{Duration, Utc};
use lightmem::{LightMem, LightMemConfig, MemoryStatus, MemoryType};
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

    // 4a. Answer (Top-1 explicit override)
    let ans = lm
        .answer_with_reranker("what port does redis use?", None, None, 5, Some("top1"))
        .expect("answer failed");
    assert_eq!(ans.reranker_used, "top1");
    assert!(ans.selected_memory.is_some());

    // 4b. Answer (Default ONNX Extractive QA slot extraction)
    let ans_prec = lm
        .answer("what port does redis use?", None, None, 5, false)
        .expect("precision answer failed");
    if std::env::var_os("LIGHTMEM_QA_DISABLE").is_some() {
        assert_eq!(ans_prec.reranker_used, "none");
    } else {
        assert_eq!(ans_prec.reranker_used, "minilm-squad2");
        assert_eq!(ans_prec.answer, "6379");
    }

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

#[test]
fn test_password_category() {
    use std::str::FromStr;

    // Test FromStr and as_str
    assert_eq!(
        MemoryType::from_str("password").unwrap(),
        MemoryType::Password
    );
    assert_eq!(
        MemoryType::from_str("passwords").unwrap(),
        MemoryType::Password
    );
    assert_eq!(MemoryType::Password.as_str(), "password");

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("password_test.db");

    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };

    let lm = LightMem::open_at(&db_path, config).expect("Failed to open LightMem");

    // Remember a password
    let mem = lm
        .remember(
            "ghp_test_secret_token_1234567890",
            Some(MemoryType::Password),
            Some("GitHub Token".to_string()),
            vec!["github".to_string(), "token".to_string(), "api".to_string()],
            Some(1.0),
        )
        .expect("remember password failed");

    assert_eq!(mem.category, MemoryType::Password);

    // Recall filtering by Password category
    let recalled = lm
        .recall(
            "GitHub secret token",
            Some(MemoryType::Password),
            None,
            5,
            None,
        )
        .expect("recall failed");
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].memory.id, mem.id);
    assert_eq!(recalled[0].memory.category, MemoryType::Password);

    // Verify stats
    let stats = lm.stats().expect("stats failed");
    let pass_count = stats
        .by_category
        .iter()
        .find(|(cat, _)| cat == "password")
        .map(|(_, count)| *count)
        .unwrap_or(0);
    assert_eq!(pass_count, 1);

    // Export OKF and verify Password category heading
    let okf_path = dir.path().join("passwords.okf");
    let exported = lm.export_okf(Some(&okf_path)).expect("export failed");
    let okf_content = std::fs::read_to_string(&exported).expect("read okf failed");
    assert!(okf_content.contains("## PASSWORD"));
    assert!(okf_content.contains("### GitHub Token"));

    // Import OKF back
    let imported_count = lm.import_file(&exported).expect("import failed");
    assert_eq!(imported_count, 1);
}

#[test]
fn test_universal_json_importers() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("import_test.db");

    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).expect("Failed to open LightMem");

    // 1. Mem0 format (array of objects with 'memory' and nested 'metadata')
    let mem0_path = dir.path().join("mem0.json");
    let mem0_json = r#"[
        {
            "id": "mem0-1",
            "memory": "User prefers dark mode and JetBrains Mono font",
            "metadata": {
                "category": "preference",
                "tags": ["theme", "editor"]
            }
        },
        {
            "id": "mem0-2",
            "memory": "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "metadata": {
                "category": "password"
            }
        }
    ]"#;
    std::fs::write(&mem0_path, mem0_json).unwrap();
    let imported_mem0 = lm.import_file(&mem0_path).expect("Mem0 import failed");
    assert_eq!(imported_mem0, 2);

    // Verify verbatim password content preservation
    let pass_mem = lm
        .recall(
            "AWS_SECRET_ACCESS_KEY",
            Some(MemoryType::Password),
            None,
            1,
            None,
        )
        .unwrap();
    assert!(!pass_mem.is_empty());
    assert_eq!(
        pass_mem[0].memory.content,
        "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
    assert_eq!(pass_mem[0].memory.category, MemoryType::Password);

    // 2. Wrapped JSON object format (e.g. {"memories": [...]})
    let wrapped_path = dir.path().join("wrapped.json");
    let wrapped_json = r#"{
        "status": "success",
        "memories": [
            {
                "title": "Postgres DB URI",
                "content": "postgres://admin:secret123@localhost:5432/mydb",
                "category": "credential",
                "tags": ["database", "postgres"]
            }
        ]
    }"#;
    std::fs::write(&wrapped_path, wrapped_json).unwrap();
    let imported_wrapped = lm
        .import_file(&wrapped_path)
        .expect("Wrapped JSON import failed");
    assert_eq!(imported_wrapped, 1);

    // 3. Line-delimited JSONL format
    let jsonl_path = dir.path().join("stream.jsonl");
    let jsonl_content = "{\"text\": \"Always run cargo test before git commit\", \"title\": \"Commit Checklist\"}\n{\"document\": \"Team decided to adopt Rust 2024 edition in Q2\", \"type\": \"decision\", \"tags\": \"rust, architecture\"}\n";
    std::fs::write(&jsonl_path, jsonl_content).unwrap();
    let imported_jsonl = lm.import_file(&jsonl_path).expect("JSONL import failed");
    assert_eq!(imported_jsonl, 2);

    // 4. Single raw JSON object
    let single_path = dir.path().join("single.json");
    let single_json = r#"{
        "content": "Deploying with systemd service on port 8080",
        "category": "runbook"
    }"#;
    std::fs::write(&single_path, single_json).unwrap();
    let imported_single = lm
        .import_file(&single_path)
        .expect("Single JSON import failed");
    assert_eq!(imported_single, 1);

    // 5. Pure-Rust heuristic extraction for unstructured arbitrary JSON keys
    let unstructured_path = dir.path().join("unstructured.json");
    let unstructured_json = r#"[
        {
            "unstructured_log": "We decided to migrate to Postgres 16 next Monday",
            "department": "Engineering"
        }
    ]"#;
    std::fs::write(&unstructured_path, unstructured_json).unwrap();
    let imported_unstructured = lm
        .import_file(&unstructured_path)
        .expect("Unstructured import failed");
    assert_eq!(imported_unstructured, 1);

    let stats = lm.stats().unwrap();
    assert_eq!(stats.total_memories, 7);
}

#[test]
fn test_pagination_and_counts() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("pagination_test.db");

    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).expect("Failed to open LightMem");

    // Store 7 memories across two categories
    for i in 1..=5 {
        lm.remember(
            &format!("Fact memory item {}", i),
            Some(MemoryType::Fact),
            Some(format!("Fact {}", i)),
            vec!["fact".to_string()],
            Some(0.9),
        )
        .unwrap();
    }
    for i in 1..=2 {
        lm.remember(
            &format!("Decision memory item {}", i),
            Some(MemoryType::Decision),
            Some(format!("Decision {}", i)),
            vec!["decision".to_string()],
            Some(0.95),
        )
        .unwrap();
    }

    // 1. Total count & filtered count
    assert_eq!(lm.count(None, Some(MemoryStatus::Active), None).unwrap(), 7);
    assert_eq!(
        lm.count(Some(MemoryType::Fact), Some(MemoryStatus::Active), None)
            .unwrap(),
        5
    );
    assert_eq!(
        lm.count(Some(MemoryType::Decision), Some(MemoryStatus::Active), None)
            .unwrap(),
        2
    );

    // 2. Offset pagination (limit = 3, offset = 0 -> 3 items, page 1/3, has_more = true)
    let page1 = lm
        .list_paginated(None, Some(MemoryStatus::Active), None, 3, 0)
        .unwrap();
    assert_eq!(page1.total, 7);
    assert_eq!(page1.items.len(), 3);
    assert_eq!(page1.offset, 0);
    assert_eq!(page1.page, 1);
    assert_eq!(page1.total_pages, 3);
    assert!(page1.has_more);

    // 3. Page 2 via list_page (page = 2, per_page = 3 -> offset 3, 3 items, has_more = true)
    let page2 = lm
        .list_page(None, Some(MemoryStatus::Active), None, 2, 3)
        .unwrap();
    assert_eq!(page2.total, 7);
    assert_eq!(page2.items.len(), 3);
    assert_eq!(page2.offset, 3);
    assert_eq!(page2.page, 2);
    assert_eq!(page2.total_pages, 3);
    assert!(page2.has_more);
    assert_ne!(page1.items[0].id, page2.items[0].id);

    // 4. Page 3 via list_page (page = 3, per_page = 3 -> offset 6, 1 item, has_more = false)
    let page3 = lm
        .list_page(None, Some(MemoryStatus::Active), None, 3, 3)
        .unwrap();
    assert_eq!(page3.total, 7);
    assert_eq!(page3.items.len(), 1);
    assert_eq!(page3.offset, 6);
    assert_eq!(page3.page, 3);
    assert_eq!(page3.total_pages, 3);
    assert!(!page3.has_more);
}

#[test]
fn test_auto_categorization_all_14_types() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("categorization_test.db");

    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).expect("Failed to open LightMem");

    let examples: Vec<(&str, MemoryType)> = vec![
        (
            "Production Stripe API key is sk_live_9988776655",
            MemoryType::Password,
        ),
        (
            "Fix panic and connection timeout bug when SQLite pool is exhausted",
            MemoryType::Error,
        ),
        (
            "How to deploy: step 1 run cargo build --release, step 2 restart systemd",
            MemoryType::Instruction,
        ),
        (
            "We decided to migrate from Python wrappers to pure Rust single crate",
            MemoryType::Decision,
        ),
        (
            "Always prefer concise single-line answers with zero fluff",
            MemoryType::Preference,
        ),
        (
            "Q4 roadmap objective: achieve sub-5ms P95 hybrid search latency",
            MemoryType::Goal,
        ),
        (
            "Deadline is Friday at 18:00, promised to deliver the benchmark report",
            MemoryType::Commitment,
        ),
        (
            "Learned that SQLite FTS5 external content triggers require exact rowid sync",
            MemoryType::Learning,
        ),
        (
            "Incident occurred on 2026-10-01 during the v0.2.0 production release",
            MemoryType::Event,
        ),
        (
            "Arjun is the lead backend engineer and reports to Priya on the infra team",
            MemoryType::Relationship,
        ),
        (
            "Observed that RSS memory stays flat at 218 MB after 1000 ONNX queries",
            MemoryType::Observation,
        ),
        (
            "Architecture specification saved in /Users/krishnakanth/Projects/lightmem/README.md",
            MemoryType::Artifact,
        ),
        (
            "Currently working on the macOS arm64 local environment setup",
            MemoryType::Context,
        ),
        (
            "Kokoro TTS service runs inside a Linux Docker container on port 8880",
            MemoryType::Fact,
        ),
    ];

    for (text, expected_category) in &examples {
        // Call remember with category = None to test automatic categorization!
        let record = lm
            .remember(text, None, None, vec![], Some(0.9))
            .expect("Failed to store auto-categorized memory");
        assert_eq!(
            record.category, *expected_category,
            "Failed auto-categorization for input: '{}'. Expected {:?}, got {:?}",
            text, expected_category, record.category
        );
    }

    let stats = lm.stats().unwrap();
    assert_eq!(stats.total_memories, 14);
    assert_eq!(
        stats.by_category.len(),
        14,
        "All 14 categories should have 1 memory each"
    );
}

#[test]
fn test_smart_merge_deduplication() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(tmp.path(), config).expect("Failed to open temp LightMem");

    // 1. Insert initial record with explicit title and tags ["profile", "legal-name"]
    let first = lm
        .remember(
            "The user's name is Krishna Kanth.",
            Some(MemoryType::Fact),
            Some("User Identity".to_string()),
            vec!["profile".to_string(), "legal-name".to_string()],
            Some(0.85),
        )
        .unwrap();

    // 2. Insert duplicate content with auto-title, higher confidence (0.98), and new tag ["kk"]
    let _second = lm
        .remember(
            "The user's name is Krishna Kanth.",
            Some(MemoryType::Fact),
            None,
            vec!["kk".to_string(), "profile".to_string()],
            Some(0.98),
        )
        .unwrap();

    // Verify only 1 active record exists and its fields were smart-merged!
    let all = lm.list(None, None, None, 10).unwrap();
    assert_eq!(
        all.len(),
        1,
        "Duplicate active record should be smart-merged into 1"
    );
    let merged = &all[0];
    assert_eq!(
        merged.title, "User Identity",
        "Explicit title should be preserved over auto-title"
    );
    assert!(
        (merged.confidence - 0.98).abs() < 1e-4,
        "Max confidence should be preserved"
    );
    assert_eq!(
        merged.created_at, first.created_at,
        "Earliest created_at should be preserved"
    );
    assert!(merged.tags.contains(&"profile".to_string()));
    assert!(merged.tags.contains(&"legal-name".to_string()));
    assert!(merged.tags.contains(&"kk".to_string()));
    assert_eq!(
        merged.tags.len(),
        3,
        "Tags should be unioned without duplicates"
    );
}

#[test]
fn test_ttl_auto_expiration() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(tmp.path(), config).unwrap();

    assert_eq!(
        lightmem::parse_ttl_duration("30s").unwrap(),
        chrono::Duration::seconds(30)
    );
    assert_eq!(
        lightmem::parse_ttl_duration("24h").unwrap(),
        chrono::Duration::hours(24)
    );
    assert_eq!(
        lightmem::parse_ttl_duration("7d").unwrap(),
        chrono::Duration::days(7)
    );

    // 1. Store a permanent memory and a short-lived TTL memory (already due)
    let (_perm, _) = lm
        .remember_with_options(
            "Permanent architecture decision: SQLite WAL mode.",
            Some(MemoryType::Decision),
            None,
            vec![],
            Some(0.95),
            None,
            false,
        )
        .unwrap();

    let (ephemeral, _) = lm
        .remember_with_options(
            "Temporary staging token expires soon.",
            Some(MemoryType::Context),
            None,
            vec!["ephemeral".to_string()],
            Some(0.9),
            Some(chrono::Duration::milliseconds(-100)),
            false,
        )
        .unwrap();
    assert!(ephemeral.expired_at.is_some());

    // 2. Querying active memories or stats should auto-transition due TTL memories to Expired
    let active = lm.list(None, Some(MemoryStatus::Active), None, 10).unwrap();
    assert_eq!(active.len(), 1);
    assert!(active[0].content.contains("SQLite WAL mode"));

    let expired_record = lm.get(&ephemeral.id).unwrap().unwrap();
    assert_eq!(expired_record.status, MemoryStatus::Expired);

    let stats = lm.stats().unwrap();
    assert_eq!(stats.total_memories, 2);
    assert_eq!(stats.active_memories, 1);
    assert_eq!(stats.expired_memories, 1);
}

#[test]
fn test_contradiction_detection_and_supersede() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(tmp.path(), config).unwrap();

    // 1. Store initial port fact
    let (old_pg, conflicts_0) = lm
        .remember_with_options(
            "PostgreSQL runs on port 5432.",
            Some(MemoryType::Fact),
            Some("Postgres Port".to_string()),
            vec![],
            Some(0.9),
            None,
            false,
        )
        .unwrap();
    assert!(conflicts_0.is_empty());

    // 2. Store conflicting port fact WITHOUT --supersede -> conflict detected, both remain active
    let (new_pg, conflicts_1) = lm
        .remember_with_options(
            "PostgreSQL runs on port 6432.",
            Some(MemoryType::Fact),
            Some("Postgres Port Updated".to_string()),
            vec![],
            Some(0.95),
            None,
            false,
        )
        .unwrap();
    assert_eq!(conflicts_1.len(), 1);
    assert!(conflicts_1[0].similarity > 0.5);
    assert!(conflicts_1[0].overlap_ratio >= 0.45);
    assert_eq!(conflicts_1[0].older_memory.id, old_pg.id);
    assert_eq!(conflicts_1[0].newer_memory.id, new_pg.id);

    // 3. Resolve conflicts via find_conflicts(true)
    let resolved = lm.find_conflicts(true).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(
        lm.get(&old_pg.id).unwrap().unwrap().status,
        MemoryStatus::Expired
    );
    assert_eq!(
        lm.get(&new_pg.id).unwrap().unwrap().status,
        MemoryStatus::Active
    );

    // 4. Test immediate --supersede on remember_with_options
    let (old_redis, _) = lm
        .remember_with_options(
            "Redis cache listens on port 6379.",
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
        )
        .unwrap();
    let (new_redis, redis_conflicts) = lm
        .remember_with_options(
            "Redis cache listens on port 6380.",
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.95),
            None,
            true,
        )
        .unwrap();
    assert_eq!(redis_conflicts.len(), 1);
    assert_eq!(
        lm.get(&old_redis.id).unwrap().unwrap().status,
        MemoryStatus::Expired
    );
    assert_eq!(
        lm.get(&new_redis.id).unwrap().unwrap().status,
        MemoryStatus::Active
    );
}

#[test]
fn test_date_filtering_in_list_and_recall() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let config = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(tmp.path(), config).unwrap();

    let (mem, _) = lm
        .remember_with_options(
            "Kafka broker runs on port 9092.",
            Some(MemoryType::Fact),
            None,
            vec!["kafka".to_string()],
            Some(0.9),
            None,
            false,
        )
        .unwrap();

    let today = mem.created_at.date_naive();
    let other_day = today - Duration::days(5);

    let today_list = lm
        .list_paginated_with_date(None, Some(MemoryStatus::Active), None, Some(today), 10, 0)
        .unwrap();
    assert_eq!(today_list.total, 1);

    let other_list = lm
        .list_paginated_with_date(
            None,
            Some(MemoryStatus::Active),
            None,
            Some(other_day),
            10,
            0,
        )
        .unwrap();
    assert_eq!(other_list.total, 0);

    let today_recall = lm
        .recall_with_date("Kafka broker", None, None, Some(today), 5, None)
        .unwrap();
    assert_eq!(today_recall.len(), 1);

    let other_recall = lm
        .recall_with_date("Kafka broker", None, None, Some(other_day), 5, None)
        .unwrap();
    assert!(other_recall.is_empty());
}
