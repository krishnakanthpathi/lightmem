use lightmem::{
    EmbeddingProvider, Exporter, HashEmbeddingProvider, HybridSearchEngine, LightMem,
    LightMemConfig, MemoryRecord, MemoryStatus, MemoryType, Storage,
};
use tempfile::tempdir;

fn record(id: &str, content: &str) -> MemoryRecord {
    let mut m = MemoryRecord::new(
        MemoryType::Fact,
        content.into(),
        content.into(),
        vec![],
        0.9,
        None,
    );
    m.id = id.into();
    m
}
fn hash_config() -> LightMemConfig {
    LightMemConfig {
        backend: "hash".into(),
        ..Default::default()
    }
}
struct Fixed(Vec<f32>);
impl EmbeddingProvider for Fixed {
    fn name(&self) -> &str {
        "fixture"
    }
    fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.0.clone())
    }
}
struct Failing;
impl EmbeddingProvider for Failing {
    fn name(&self) -> &str {
        "failing"
    }
    fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("synthetic provider failure")
    }
}

#[test]
fn legacy_index_requires_migration_and_reports_progress() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let storage = Storage::open(&db).unwrap();
    storage
        .insert_memory(&record("one", "Redis runs on port 6379"), Some(&[1.0, 0.0]))
        .unwrap();
    drop(storage);
    let lm = LightMem::open_at(&db, hash_config()).unwrap();
    assert!(lm.embedding_migration_needed().unwrap());
    assert!(lm
        .recall("Redis", None, None, 5, None)
        .unwrap_err()
        .to_string()
        .contains("migration required"));
    let mut events = Vec::new();
    assert_eq!(
        lm.reindex(|done, total| events.push((done, total)))
            .unwrap(),
        1
    );
    assert_eq!(events, vec![(0, 1), (1, 1)]);
    assert!(!lm.embedding_migration_needed().unwrap());
    assert_eq!(
        lm.recall("Redis", None, None, 5, None).unwrap()[0]
            .memory
            .id,
        "one"
    );
}

#[test]
fn same_dimensions_do_not_make_different_models_compatible() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .insert_indexed_batch(
            &[record("one", "hello")],
            &Fixed(vec![1.0, 0.0]),
            "model-A",
            true,
        )
        .unwrap();
    assert!(storage.check_embedding_identity("model-B").is_err());
    assert!(storage
        .insert_indexed_batch(
            &[record("two", "world")],
            &Fixed(vec![1.0, 0.0]),
            "model-B",
            true
        )
        .is_err());
    assert_eq!(storage.stats().unwrap().total_memories, 1);
    let (revision, _) = storage.index_snapshot().unwrap();
    storage
        .replace_index(revision, "model-B", &[("one".into(), vec![0.0, 1.0])])
        .unwrap();
    assert!(storage.check_embedding_identity("model-B").is_ok());
    assert!(storage.check_embedding_identity("model-A").is_err());
}

#[test]
fn index_commit_rejects_bad_vectors_and_preserves_previous_index() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .insert_indexed_batch(
            &[record("one", "one"), record("two", "two")],
            &Fixed(vec![1.0, 0.0]),
            "old",
            true,
        )
        .unwrap();
    let (revision, _) = storage.index_snapshot().unwrap();
    assert!(storage
        .replace_index(
            revision,
            "new",
            &[("one".into(), vec![2.0]), ("two".into(), vec![f32::NAN])]
        )
        .is_err());
    assert_eq!(
        storage.embedding_identity().unwrap().as_deref(),
        Some("old")
    );
    let vectors = storage.get_candidate_vectors(None, None, None).unwrap();
    assert!(vectors.iter().all(|(_, v)| v == &[1.0, 0.0]));
    assert!(storage
        .replace_index(revision, "new", &[("one".into(), vec![2.0])])
        .is_err());
}

#[test]
fn migration_rejects_concurrent_writes_and_retains_old_index() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let lm = LightMem::open_at(&db, hash_config()).unwrap();
    let first = lm
        .remember("first memory", None, None, vec![], None)
        .unwrap();
    let other = LightMem::open_at(&db, hash_config()).unwrap();
    let result = lm.reindex(|done, total| {
        if done == total && done > 0 {
            other
                .remember("concurrent memory", None, None, vec![], None)
                .unwrap();
        }
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("changed during migration"));
    assert_eq!(lm.stats().unwrap().total_vectors, 2);
    assert!(lm.get(&first.id).unwrap().is_some());
    assert!(!lm.embedding_migration_needed().unwrap());
}

#[test]
fn batch_embedding_failure_rolls_back_memories_and_metadata() {
    let storage = Storage::open_in_memory().unwrap();
    assert!(storage
        .insert_indexed_batch(&[record("one", "hello")], &Failing, "fixture", true)
        .is_err());
    assert_eq!(storage.stats().unwrap().total_memories, 0);
    assert_eq!(storage.stats().unwrap().total_vectors, 0);
    assert_eq!(storage.embedding_identity().unwrap(), None);
}

#[test]
fn dedup_keeps_id_and_reembeds_final_merged_card() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let lm = LightMem::open_at(&db, hash_config()).unwrap();
    let first = lm
        .remember(
            "Redis runs on port 6379",
            Some(MemoryType::Fact),
            Some("Redis configuration".into()),
            vec!["cache".into()],
            Some(0.8),
        )
        .unwrap();
    let merged = lm
        .remember(
            "Redis runs on port 6379",
            Some(MemoryType::Fact),
            None,
            vec!["production".into()],
            Some(0.95),
        )
        .unwrap();
    assert_eq!(merged.id, first.id);
    assert_eq!(merged.title, "Redis configuration");
    assert_eq!(merged.confidence, 0.95);
    assert_eq!(merged.tags.len(), 2);
    let storage = Storage::open(db).unwrap();
    let vectors = storage.get_candidate_vectors(None, None, None).unwrap();
    assert_eq!(vectors.len(), 1);
    assert_eq!(
        vectors[0].1,
        HashEmbeddingProvider.embed(&merged.to_card_text()).unwrap()
    );
}

#[test]
fn dedup_is_conservative_for_case_whitespace_category_and_paraphrases() {
    let storage = Storage::open_in_memory().unwrap();
    let mut records = vec![
        record("one", "Token ABC"),
        record("two", "Token abc"),
        record("three", "Token  ABC"),
        record("four", "Token ABC"),
    ];
    records[3].category = MemoryType::Password;
    records.push(record("five", "Redis port is 6379"));
    records.push(record("six", "Redis uses port 6379"));
    storage
        .insert_indexed_batch(&records, &HashEmbeddingProvider, "hash", false)
        .unwrap();
    assert_eq!(
        storage
            .deduplicate_indexed(&HashEmbeddingProvider, "hash")
            .unwrap(),
        0
    );
    assert_eq!(storage.stats().unwrap().total_memories, 6);
}

#[test]
fn explicit_dedup_merges_all_duplicates_and_refreshes_vector() {
    let storage = Storage::open_in_memory().unwrap();
    let mut a = record("one", "Redis runs on port 6379");
    a.tags = vec!["cache".into()];
    let mut b = record("two", "Redis runs on port 6379");
    b.tags = vec!["prod".into()];
    storage
        .insert_indexed_batch(&[a.clone(), b], &HashEmbeddingProvider, "hash", false)
        .unwrap();
    assert_eq!(
        storage
            .deduplicate_indexed(&HashEmbeddingProvider, "hash")
            .unwrap(),
        1
    );
    assert_eq!(
        storage
            .deduplicate_indexed(&HashEmbeddingProvider, "hash")
            .unwrap(),
        0
    );
    let survivor = storage.get_memory("one").unwrap().unwrap();
    assert_eq!(survivor.tags, vec!["cache", "prod"]);
    assert_eq!(
        storage.get_candidate_vectors(None, None, None).unwrap()[0].1,
        HashEmbeddingProvider
            .embed(&survivor.to_card_text())
            .unwrap()
    );
    assert!(storage.get_memory("two").unwrap().is_none());
}

#[test]
fn exact_ids_win_and_prefix_wildcards_are_literal() {
    let storage = Storage::open_in_memory().unwrap();
    for m in [
        record("abc-long", "long"),
        record("abc", "exact"),
        record("a%literal", "literal"),
    ] {
        storage.insert_memory(&m, None).unwrap();
    }
    assert_eq!(storage.get_memory("abc").unwrap().unwrap().content, "exact");
    assert!(storage.forget_memory("", true).is_err());
    assert!(!storage.forget_memory("%", true).unwrap());
    assert_eq!(storage.get_memory("a%").unwrap().unwrap().id, "a%literal");
}

#[test]
fn backup_restores_all_fields_expired_records_and_duplicate_ids() {
    let dir = tempdir().unwrap();
    let source = Storage::open_in_memory().unwrap();
    let mut a = record(
        "one",
        "  ## Heading\n---\n```\n<!-- lightmem-record evil -->\n  ",
    );
    a.title = "Title\nline".into();
    a.tags = vec!["comma,inside".into(), "unicode-λ".into()];
    a.status = MemoryStatus::Expired;
    a.expired_at = Some(a.updated_at);
    a.provenance = "external:fixture".into();
    let mut b = a.clone();
    b.id = "two".into();
    source
        .insert_indexed_batch(
            &[a.clone(), b.clone()],
            &HashEmbeddingProvider,
            "hash",
            false,
        )
        .unwrap();
    let path = dir.path().join("backup.json");
    Exporter::export_json(&source, &path).unwrap();
    let target = LightMem::open_at(&dir.path().join("target.db"), hash_config()).unwrap();
    assert_eq!(target.import_file(&path).unwrap(), 2);
    for record in [a, b] {
        assert_eq!(
            serde_json::to_value(target.get(&record.id).unwrap().unwrap()).unwrap(),
            serde_json::to_value(record).unwrap()
        );
    }
    assert_eq!(target.stats().unwrap().expired_memories, 2);
}

#[test]
fn invalid_jsonl_rolls_back_entire_directory() {
    let dir = tempdir().unwrap();
    let files = dir.path().join("import");
    std::fs::create_dir(&files).unwrap();
    std::fs::write(files.join("a.json"), r#"{"content":"valid first record"}"#).unwrap();
    std::fs::write(
        files.join("b.jsonl"),
        "{\"content\":\"valid line\"}\ninvalid json",
    )
    .unwrap();
    let lm = LightMem::open_at(&dir.path().join("target.db"), hash_config()).unwrap();
    assert!(lm.import_file(&files).is_err());
    assert_eq!(lm.stats().unwrap().total_memories, 0);
}

#[test]
fn rrf_final_ranking_preserves_keyword_advantage() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .insert_memory(&record("keyword", "target"), Some(&[0.2, 0.98]))
        .unwrap();
    storage
        .insert_memory(&record("vector", "unrelated text"), Some(&[1.0, 0.0]))
        .unwrap();
    let results = HybridSearchEngine::search(
        &storage,
        &Fixed(vec![1.0, 0.0]),
        "target",
        None,
        None,
        None,
        2,
        None,
    )
    .unwrap();
    assert_eq!(results[0].memory.id, "keyword");
    assert_eq!(results[0].bm25_rank, Some(1));
    assert!(results[0].score > results[1].score);
}

#[test]
fn empty_vault_can_bind_a_new_model_after_last_memory_is_deleted() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .insert_indexed_batch(&[record("one", "one")], &Fixed(vec![1.0, 0.0]), "old", true)
        .unwrap();
    storage.forget_memory("one", true).unwrap();
    storage
        .insert_indexed_batch(&[record("two", "two")], &Fixed(vec![1.0]), "new", true)
        .unwrap();
    assert_eq!(
        storage.embedding_identity().unwrap().as_deref(),
        Some("new")
    );
}

#[test]
fn legacy_schema_migration_repairs_stale_fts_and_is_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    {
        let storage = Storage::open(&path).unwrap();
        storage
            .insert_memory(&record("one", "currentword"), None)
            .unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO memories_fts(id,title,content,tags) VALUES('one','stale','oldword','')",
            [],
        )
        .unwrap();
        conn.execute("UPDATE memories SET tags='alpha,beta'", [])
            .unwrap();
        conn.execute(
            "INSERT INTO memories_fts(id,title,content,tags) VALUES('one','stale','oldword','')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 0).unwrap();
    }
    for _ in 0..2 {
        let storage = Storage::open(&path).unwrap();
        assert!(storage
            .search_bm25("oldword", None, None, None, 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            storage
                .search_bm25("currentword", None, None, None, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            storage.get_memory("one").unwrap().unwrap().tags,
            vec!["alpha", "beta"]
        );
    }
}

#[test]
fn batch_updates_can_merge_an_earlier_record_into_an_existing_id() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .insert_indexed_batch(
            &[record("existing", "old content")],
            &HashEmbeddingProvider,
            "hash",
            true,
        )
        .unwrap();
    let results = storage
        .insert_indexed_batch(
            &[
                record("new", "shared content"),
                record("existing", "shared content"),
            ],
            &HashEmbeddingProvider,
            "hash",
            true,
        )
        .unwrap();
    assert!(results.iter().all(|m| m.id == "existing"));
    assert_eq!(storage.stats().unwrap().total_memories, 1);
    assert_eq!(storage.stats().unwrap().total_vectors, 1);
}

#[test]
fn okf_v2_roundtrip_preserves_arbitrary_markdown_and_metadata() {
    let storage = Storage::open_in_memory().unwrap();
    let mut m = record("id\nwith newline", " leading\n## heading\n### heading\n---\n```rust\ncode\n```\n<!-- lightmem-record bad -->\ntrailing  ");
    m.tags = vec![
        "comma,inside".into(),
        "tag\n<!-- lightmem-record bad -->".into(),
    ];
    m.title = "Title\nwith newline".into();
    storage
        .insert_indexed_batch(&[m.clone()], &HashEmbeddingProvider, "hash", false)
        .unwrap();
    let dir = tempdir().unwrap();
    let path = dir.path().join("export.md");
    Exporter::export_okf(&storage, Some(&path)).unwrap();
    use lightmem::MemoryImporter;
    let imported = lightmem::OkfMemoryImporter
        .parse(&std::fs::read_to_string(path).unwrap())
        .unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(
        serde_json::to_value(imported.into_iter().next().unwrap().to_memory_record()).unwrap(),
        serde_json::to_value(m).unwrap()
    );
}
