use chrono::{DateTime, Duration, Utc};
use lightmem::models::{LightMemConfig, MemoryConflict, MemoryRecord, MemoryStatus, MemoryType};
use lightmem::services::embeddings::MultimodalInput;
use lightmem::services::{Reranker, Top1Reranker};
use lightmem::storage::Storage;
use lightmem::LightMem;

fn record(content: &str, category: MemoryType, at: DateTime<Utc>) -> MemoryRecord {
    let mut memory = MemoryRecord::new(category, content.into(), content.into(), vec![], 0.9, None);
    memory.created_at = at;
    memory.updated_at = at;
    memory
}

fn open(path: &std::path::Path) -> LightMem {
    LightMem::open_at(
        path,
        LightMemConfig {
            backend: "hash".into(),
            reranker: "top1".into(),
            ..LightMemConfig::default()
        },
    )
    .unwrap()
}

#[test]
fn day_recall_exhausts_candidates_beyond_global_prefix_for_text_and_multimodal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("day.db");
    let now = Utc::now();
    let day = now - Duration::days(3);
    let target = record("quartz", MemoryType::Fact, day);
    let storage = Storage::open(&path).unwrap();
    for i in 0..270 {
        storage
            .insert_memory(
                &record(&format!("quartz signal sample {i}"), MemoryType::Fact, now),
                None,
            )
            .unwrap();
    }
    storage.insert_memory(&target, None).unwrap();
    drop(storage);
    let mem = open(&path);
    mem.reindex(|_, _| {}).unwrap();
    let global = mem.recall("quartz signal", None, None, 250, None).unwrap();
    assert_eq!(global.len(), 250);
    assert!(
        !global.iter().any(|h| h.memory.id == target.id),
        "fixture target must be outside global prefix"
    );
    let text = mem
        .recall_with_date("quartz", None, None, Some(day.date_naive()), 1, None)
        .unwrap();
    assert_eq!(
        text.len(),
        1,
        "day recall must not inspect only the global top 250"
    );
    assert_eq!(text[0].memory.id, target.id);
    let media = mem
        .recall_multimodal_expanded(
            &MultimodalInput::text_only("quartz"),
            None,
            None,
            Some(day.date_naive()),
            1,
            None,
            false,
        )
        .unwrap();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].memory.id, target.id);
}

#[test]
fn local_conflict_merge_allows_supersede_preserving_independent_claims() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("merge.db");
    let now = Utc::now();
    let older = record(
        "Postgres runs on port 5432; backups are enabled.",
        MemoryType::Fact,
        now - Duration::days(1),
    );
    let newer = record("Postgres runs on port 6432.", MemoryType::Fact, now);
    // Local fallback merges: newer wins, preserving older attachments
    let merged = Top1Reranker.merge_conflict(&older, &newer).unwrap();
    assert_eq!(merged.content, "Postgres runs on port 6432.");
    let storage = Storage::open(&path).unwrap();
    storage.insert_memory(&older, None).unwrap();
    storage.insert_memory(&newer, None).unwrap();
    drop(storage);
    let mem = open(&path);
    mem.reindex(|_, _| {}).unwrap();
    let pair = MemoryConflict {
        older_memory: older.clone(),
        newer_memory: newer.clone(),
        similarity: 0.9,
        overlap_ratio: 0.8,
    };
    assert!(mem.merge_conflict_pair(&pair, Some("top1")).is_ok());
    let reloaded = mem.get(&older.id).unwrap();
    // Older should be soft-retired after merge
    assert!(reloaded.is_none() || reloaded.unwrap().status == MemoryStatus::Expired);
    assert_eq!(mem.get(&newer.id).unwrap().unwrap().content, newer.content);
    let same = record(&older.content, MemoryType::Fact, now);
    assert_eq!(
        Top1Reranker.merge_conflict(&older, &same).unwrap().content,
        older.content
    );
}
