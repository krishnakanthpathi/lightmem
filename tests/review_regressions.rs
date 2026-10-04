use lightmem::{Exporter, MemoryImporter, MemoryRecord, MemoryType, OkfMemoryImporter, Storage};

fn record(content: &str) -> MemoryRecord {
    MemoryRecord::new(
        MemoryType::Fact,
        "Test".into(),
        content.into(),
        vec![],
        0.9,
        None,
    )
}

#[test]
fn replacing_same_id_should_remove_old_fts_terms() {
    let storage = Storage::open_in_memory().unwrap();
    let mut memory = record("olduniqueterm");
    storage.insert_memory(&memory, None).unwrap();
    memory.content = "newuniqueterm".into();
    storage.insert_memory(&memory, None).unwrap();
    assert!(storage
        .search_bm25("olduniqueterm", None, None, None, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn markdown_body_should_roundtrip() {
    let storage = Storage::open_in_memory().unwrap();
    let content = "First paragraph\n---\nSecond paragraph";
    storage.insert_memory(&record(content), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.md");
    Exporter::export_okf(&storage, Some(&path)).unwrap();
    let raw = std::fs::read_to_string(path).unwrap();
    let imported = OkfMemoryImporter.parse(&raw).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].content, content);
}

#[test]
fn ambiguous_prefix_should_not_delete_arbitrary_record() {
    let storage = Storage::open_in_memory().unwrap();
    let mut first = record("first content");
    first.id = "abc-one".into();
    let mut second = record("second content");
    second.id = "abc-two".into();
    storage.insert_memory(&first, None).unwrap();
    storage.insert_memory(&second, None).unwrap();
    assert!(storage.forget_memory("abc", true).is_err());
}
