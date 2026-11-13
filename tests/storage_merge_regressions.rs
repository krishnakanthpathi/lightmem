use lightmem::models::{AttachmentInput, EntityRecord, MemoryRecord, MemoryType};
use lightmem::repositories::sqlite_repo::Storage;
use lightmem::services::embeddings::HashEmbeddingProvider;
use rusqlite::{types::Value, Connection};
use tempfile::{tempdir, TempDir};

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
    m.entities = vec![EntityRecord::new("owner", content)];
    m
}

fn fixture() -> (TempDir, Storage, Storage) {
    let dir = tempdir().unwrap();
    let target = Storage::open(dir.path().join("target.db")).unwrap();
    let source = Storage::open(dir.path().join("source.db")).unwrap();
    for (storage, text) in [(&target, "Target"), (&source, "Source")] {
        for id in ["collision", "peer"] {
            let attachment =
                AttachmentInput::from_bytes(&format!("{text}-{id}.txt"), text.as_bytes().to_vec());
            storage
                .insert_indexed_with_attachments(
                    &record(id, &format!("{text}-{id}")),
                    &[attachment],
                    &HashEmbeddingProvider,
                    "hash",
                    false,
                )
                .unwrap();
        }
    }
    source
        .insert_indexed_with_attachments(
            &record("new", "New source"),
            &[AttachmentInput::from_bytes("new.txt", b"New".to_vec())],
            &HashEmbeddingProvider,
            "hash",
            false,
        )
        .unwrap();
    source
        .add_link("collision", "peer", Some("source-only"), None)
        .unwrap();
    source
        .add_link("new", "collision", Some("mixed"), None)
        .unwrap();
    (dir, target, source)
}

fn snapshot(path: &std::path::Path, sql: &str) -> Vec<Vec<Value>> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let columns = stmt.column_count();
    stmt.query_map([], |r| (0..columns).map(|i| r.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn merge_skip_preserves_existing_entities() {
    let (dir, target, source) = fixture();
    let path = dir.path().join("target.db");
    let sql = "SELECT * FROM memory_entities WHERE memory_id IN ('collision', 'peer') ORDER BY id";
    let before = snapshot(&path, sql);
    assert_eq!(target.merge_vault(&source, false).unwrap(), 1);
    assert_eq!(snapshot(&path, sql), before);
    assert_eq!(target.get_memory("new").unwrap().unwrap().entities.len(), 1);
}

#[test]
fn merge_skip_preserves_existing_attachments() {
    let (dir, target, source) = fixture();
    let path = dir.path().join("target.db");
    let sql =
        "SELECT * FROM memory_attachments WHERE memory_id IN ('collision', 'peer') ORDER BY id";
    let before = snapshot(&path, sql);
    assert_eq!(target.merge_vault(&source, false).unwrap(), 1);
    assert_eq!(snapshot(&path, sql), before);
    assert_eq!(
        target.get_memory_attachments_data("new").unwrap()[0].1,
        b"New"
    );
}

#[test]
fn merge_skip_does_not_fill_existing_vectors() {
    let (dir, target, source) = fixture();
    let path = dir.path().join("target.db");
    Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM memory_vectors WHERE id='collision'", [])
        .unwrap();
    let sql = "SELECT * FROM memory_vectors WHERE id IN ('collision', 'peer') ORDER BY id";
    let before = snapshot(&path, sql);
    assert_eq!(target.merge_vault(&source, false).unwrap(), 1);
    assert_eq!(snapshot(&path, sql), before);
    assert_eq!(
        snapshot(&path, "SELECT id FROM memory_vectors WHERE id='new'").len(),
        1
    );
}

#[test]
fn merge_skip_does_not_add_links_to_skipped_memories() {
    let (dir, target, source) = fixture();
    let path = dir.path().join("target.db");
    let before = snapshot(&path, "SELECT * FROM memory_links");
    assert_eq!(target.merge_vault(&source, false).unwrap(), 1);
    assert_eq!(snapshot(&path, "SELECT * FROM memory_links"), before);
}

#[test]
fn merge_replace_replaces_children_and_links() {
    let (dir, target, source) = fixture();
    let path = dir.path().join("target.db");
    assert_eq!(target.merge_vault(&source, true).unwrap(), 3);
    assert_eq!(
        target.get_memory("collision").unwrap().unwrap().entities[0].val,
        "Source-collision"
    );
    assert_eq!(
        target
            .get_memory_attachments_data("collision")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        target.get_memory_attachments_data("collision").unwrap()[0].1,
        b"Source"
    );
    assert_eq!(snapshot(&path, "SELECT * FROM memory_links").len(), 2);
}
