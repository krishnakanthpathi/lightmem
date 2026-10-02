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

    let imported_count = lm.import_file(&exported, false).expect("import failed");
    assert!(imported_count >= 1);
}

#[test]
fn test_password_category() {
    use std::str::FromStr;

    // Test FromStr and as_str
    assert_eq!(MemoryType::from_str("password").unwrap(), MemoryType::Password);
    assert_eq!(MemoryType::from_str("passwords").unwrap(), MemoryType::Password);
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
        .recall("GitHub secret token", Some(MemoryType::Password), None, 5, None)
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
    let imported_count = lm.import_file(&exported, false).expect("import failed");
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
    let imported_mem0 = lm.import_file(&mem0_path, false).expect("Mem0 import failed");
    assert_eq!(imported_mem0, 2);

    // Verify verbatim password content preservation
    let pass_mem = lm.recall("AWS_SECRET_ACCESS_KEY", Some(MemoryType::Password), None, 1, None).unwrap();
    assert!(!pass_mem.is_empty());
    assert_eq!(pass_mem[0].memory.content, "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
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
    let imported_wrapped = lm.import_file(&wrapped_path, false).expect("Wrapped JSON import failed");
    assert_eq!(imported_wrapped, 1);

    // 3. Line-delimited JSONL format
    let jsonl_path = dir.path().join("stream.jsonl");
    let jsonl_content = "{\"text\": \"Always run cargo test before git commit\", \"title\": \"Commit Checklist\"}\n{\"document\": \"Team decided to adopt Rust 2024 edition in Q2\", \"type\": \"decision\", \"tags\": \"rust, architecture\"}\n";
    std::fs::write(&jsonl_path, jsonl_content).unwrap();
    let imported_jsonl = lm.import_file(&jsonl_path, false).expect("JSONL import failed");
    assert_eq!(imported_jsonl, 2);

    // 4. Single raw JSON object
    let single_path = dir.path().join("single.json");
    let single_json = r#"{
        "content": "Deploying with systemd service on port 8080",
        "category": "runbook"
    }"#;
    std::fs::write(&single_path, single_json).unwrap();
    let imported_single = lm.import_file(&single_path, false).expect("Single JSON import failed");
    assert_eq!(imported_single, 1);

    // 5. Needle SLM extraction test
    let needle_venv = "/Users/krishnakanth/Projects/needle3-mac-bench/.venv/bin/python";
    if std::path::Path::new(needle_venv).exists() {
        let needle_test_path = dir.path().join("needle_test.json");
        let needle_json = r#"[
            {
                "unstructured_log": "Meeting notes with Bob: we agreed to migrate to Postgres 16 next Monday",
                "department": "Engineering"
            }
        ]"#;
        std::fs::write(&needle_test_path, needle_json).unwrap();
        let imported_needle = lm.import_file(&needle_test_path, true).expect("Needle import failed");
        assert_eq!(imported_needle, 1);
    }

    let stats = lm.stats().unwrap();
    assert!(stats.total_memories >= 6);
}



