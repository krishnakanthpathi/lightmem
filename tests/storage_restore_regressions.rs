use lightmem::models::{EntityRecord, MemoryRecord, MemoryType};
use lightmem::repositories::sqlite_repo::Storage;
use lightmem::services::embeddings::HashEmbeddingProvider;
use tempfile::tempdir;

fn record(id: &str, content: &str) -> MemoryRecord {
    let mut memory = MemoryRecord::new(
        MemoryType::Fact,
        content.to_string(),
        content.to_string(),
        vec![],
        0.9,
        None,
    );
    memory.id = id.to_string();
    memory.entities.push(EntityRecord::new("city", content));
    memory
}

fn insert(storage: &Storage, memory: MemoryRecord) {
    storage
        .insert_indexed_batch(&[memory], &HashEmbeddingProvider, "hash", false)
        .unwrap();
}

#[test]
fn json_backup_roundtrip_preserves_structured_entities() {
    use lightmem::services::importer::JsonMemoryImporter;

    let original = record("entity-memory", "Hyderabad");
    let backup = serde_json::json!({
        "format": "lightmem-backup-v1",
        "memories": [original.clone()],
    });
    let candidate = JsonMemoryImporter
        .parse_flexible(&backup.to_string())
        .unwrap()
        .remove(0);
    let restored = candidate.into_record_and_attachments().unwrap().0;
    assert_eq!(
        serde_json::to_value(&restored.entities).unwrap(),
        serde_json::to_value(&original.entities).unwrap()
    );
    let target = Storage::open_in_memory().unwrap();
    insert(&target, restored);
    let matches = target.find_by_entity("city", Some("Hyderabad")).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, original.id);
}

#[test]
fn failed_future_schema_restore_preserves_live_database() {
    let dir = tempdir().unwrap();
    let target_path = dir.path().join("target.db");
    let source_path = dir.path().join("source.db");
    let target = Storage::open(&target_path).unwrap();
    insert(&target, record("original", "Original"));
    let source = Storage::open(&source_path).unwrap();
    insert(&source, record("incoming", "Incoming"));
    drop(source);
    rusqlite::Connection::open(&source_path)
        .unwrap()
        .pragma_update(None, "user_version", 999)
        .unwrap();

    let error = target.restore_from_db_file(&source_path).unwrap_err();
    assert!(error.to_string().contains("newer"), "{error:#}");
    assert!(
        target.get_memory("original").unwrap().is_some(),
        "failed restore lost original"
    );
    assert!(target.get_memory("incoming").unwrap().is_none());
    drop(target);
    let reopened = Storage::open(&target_path).unwrap();
    assert_eq!(
        reopened.get_memory("original").unwrap().unwrap().entities[0].val,
        "Original"
    );
    insert(&reopened, record("after", "Still writable"));
}
