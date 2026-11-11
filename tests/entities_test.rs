use lightmem::controllers::LightMem;
use lightmem::models::{EntityRecord, MemoryType};
use tempfile::tempdir;

#[test]
fn test_entities_persistence_and_lookup() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("entities_test.db");
    let config = lightmem::models::LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).unwrap();

    // 1. Remember with explicit entities
    let mut mem = lightmem::models::MemoryRecord::new(
        MemoryType::Fact,
        "User Address".to_string(),
        "I am currently living at Flat 402, Sunshine Apts, Hyderabad with pincode 500081"
            .to_string(),
        vec!["address".to_string()],
        0.95,
        None,
    );
    mem.entities.push(EntityRecord::new("pincode", "500081"));
    mem.entities.push(EntityRecord::new("city", "Hyderabad"));
    mem.entities
        .push(EntityRecord::new("apartment", "Sunshine Apts"));

    // Insert with entities populated
    let embedder = lightmem::services::embeddings::HashEmbeddingProvider;
    let identity = lm.requested_embedding_identity();
    let stored = lm
        .storage()
        .insert_indexed_batch(&[mem.clone()], &embedder, identity, true)
        .unwrap()
        .remove(0);

    // 2. Load record and verify entities are populated
    let loaded = lm.get(&stored.id).unwrap().expect("Record should exist");
    assert_eq!(loaded.entities.len(), 3);
    assert_eq!(loaded.entities[0].name, "pincode");
    assert_eq!(loaded.entities[0].val, "500081");
    assert_eq!(loaded.entities[1].name, "city");
    assert_eq!(loaded.entities[1].val, "Hyderabad");

    // 3. Find by entity lookup
    let by_pin = lm.find_by_entity("pincode", Some("500081")).unwrap();
    assert_eq!(by_pin.len(), 1);
    assert_eq!(by_pin[0].id, stored.id);

    let by_city = lm.find_by_entity("city", None).unwrap();
    assert_eq!(by_city.len(), 1);
    assert_eq!(by_city[0].id, stored.id);

    // 4. Test exact entity QA answering
    let ans = lm
        .answer("what is my pincode?", None, None, 5, false)
        .unwrap();
    assert_eq!(ans.answer, "500081");
    assert_eq!(ans.reranker_used, "entity_index");
}

#[test]
fn test_remember_with_entities_warning_when_llm_offline() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("entities_warning_test.db");
    let mut config = lightmem::models::LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    // Force invalid Ollama URL to simulate unreachable offline LLM
    config.ollama_url = "http://127.0.0.1:65534".to_string();
    let lm = LightMem::open_at(&db_path, config).unwrap();

    // With extract_entities = true (default), when LLM is offline, it should save and emit a warning
    let (record, _conflicts, warning) = lm
        .remember_with_attachments_and_entities(
            Some("Living at 123 Main Street, Boston 02101"),
            &[],
            false,
            true, // extract_entities
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
        )
        .unwrap();

    // Memory is safely persisted
    assert!(!record.id.is_empty());
    assert!(record.content.contains("123 Main Street"));

    // Warning is emitted informing the user
    assert!(
        warning.is_some(),
        "Warning should be emitted when LLM is unreachable"
    );
    let warn_str = warning.unwrap();
    assert!(
        warn_str.contains("Could not extract entities"),
        "Expected warning message: {}",
        warn_str
    );

    // With extract_entities = false (opt-out), no warning should be emitted
    let (rec2, _, warn2) = lm
        .remember_with_attachments_and_entities(
            Some("Living at 456 Oak Avenue, Seattle 98101"),
            &[],
            false,
            false, // opt out
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
        )
        .unwrap();

    assert!(!rec2.id.is_empty());
    assert!(
        warn2.is_none(),
        "No warning should be emitted when opt-out is passed"
    );
}

#[test]
fn test_entities_dedup_and_conflict_merge() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("entities_merge_test.db");
    let config = lightmem::models::LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).unwrap();
    let embedder = lightmem::services::embeddings::HashEmbeddingProvider;
    let identity = lm.requested_embedding_identity();

    let mut mem1 = lightmem::models::MemoryRecord::new(
        MemoryType::Fact,
        "User Location".to_string(),
        "I live in Hyderabad".to_string(),
        vec!["location".to_string()],
        0.9,
        None,
    );
    mem1.entities.push(EntityRecord::new("city", "Hyderabad"));
    let _stored1 = lm
        .storage()
        .insert_indexed_batch(&[mem1], &embedder, identity, true)
        .unwrap()
        .remove(0);

    let mut mem2 = lightmem::models::MemoryRecord::new(
        MemoryType::Fact,
        "User Location".to_string(),
        "I live in Hyderabad and my pincode is 500081".to_string(),
        vec!["location".to_string()],
        0.95,
        None,
    );
    mem2.entities.push(EntityRecord::new("city", "Hyderabad"));
    mem2.entities.push(EntityRecord::new("pincode", "500081"));
    let stored2 = lm
        .storage()
        .insert_indexed_batch(&[mem2], &embedder, identity, true)
        .unwrap()
        .remove(0);

    let loaded = lm.get(&stored2.id).unwrap().unwrap();
    assert_eq!(loaded.entities.len(), 2);
    assert!(loaded
        .entities
        .iter()
        .any(|e| e.name == "city" && e.val == "Hyderabad"));
    assert!(loaded
        .entities
        .iter()
        .any(|e| e.name == "pincode" && e.val == "500081"));
}

#[test]
fn test_entity_index_guards_tautology_word_boundary_and_reranker_override() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("entity_guards.db");
    let config = lightmem::models::LightMemConfig {
        backend: "hash".to_string(),
        reranker: "top1".to_string(),
        ..Default::default()
    };
    let lm = LightMem::open_at(&db_path, config).unwrap();

    let mut mem = lightmem::models::MemoryRecord::new(
        MemoryType::Decision,
        "SQLite WAL Decision".to_string(),
        "LightMem uses SQLite WAL and FTS5 for hybrid retrieval.".to_string(),
        vec!["sqlite".to_string()],
        0.95,
        None,
    );
    // Subject tautology ("lightmem" -> "LightMem"), substring trap ("id" inside "hybrid"), and pincode
    mem.entities.push(EntityRecord::new("lightmem", "LightMem"));
    mem.entities.push(EntityRecord::new("id", "99999"));
    mem.entities.push(EntityRecord::new("engine", "SQLite WAL"));

    let embedder = lightmem::services::embeddings::HashEmbeddingProvider;
    let identity = lm.requested_embedding_identity();
    lm.storage()
        .insert_indexed_batch(&[mem], &embedder, identity, true)
        .unwrap();

    // Without override: must NOT echo tautological "LightMem" or substring "id" from "hybrid"
    let ans_auto = lm
        .answer(
            "What does LightMem use for hybrid retrieval?",
            None,
            None,
            5,
            false,
        )
        .unwrap();
    assert_eq!(ans_auto.reranker_used, "top1");
    assert!(ans_auto.answer.contains("SQLite WAL"));

    // Whole-word attribute match ("engine" -> "SQLite WAL") STILL uses entity_index when not overridden
    let ans_entity = lm
        .answer("What is the storage engine?", None, None, 5, false)
        .unwrap();
    assert_eq!(ans_entity.reranker_used, "entity_index");
    assert_eq!(ans_entity.answer, "SQLite WAL");

    // Explicit reranker override MUST bypass entity_index
    let ans_override = lm
        .answer_with_reranker("What is the storage engine?", None, None, 5, Some("top1"))
        .unwrap();
    assert_eq!(ans_override.reranker_used, "top1");
    assert!(ans_override.answer.contains("SQLite WAL"));
}
