use lightmem::{LightMem, LightMemConfig, MemoryType, ObserverService};
use tempfile::tempdir;

#[test]
fn test_observer_extracts_preferences_and_facts_offline() {
    let chat = r#"
    User: Hey! Can you help me set up our dev environment?
    Assistant: Sure! What are your preferred tools?
    User: I prefer dark mode and JetBrains Mono font.
    Assistant: Got it. I'll configure that.
    User: We decided to deploy PostgreSQL 16 on port 5433 instead of the default port.
    Assistant: Perfect, noted.
    User: Always ensure strict clippy checks pass in CI.
    "#;

    let candidates = ObserverService::extract(chat, None, None, Some(0.80), None, None)
        .expect("Extraction should succeed");

    assert!(
        candidates.len() >= 3,
        "Expected at least 3 candidates, found {}",
        candidates.len()
    );

    let pref = candidates
        .iter()
        .find(|c| c.category == MemoryType::Preference);
    assert!(pref.is_some(), "Preference should be detected");
    assert!(pref.unwrap().content.to_lowercase().contains("dark mode"));

    let decision = candidates
        .iter()
        .find(|c| c.category == MemoryType::Decision);
    assert!(decision.is_some(), "Decision should be detected");
    assert!(decision.unwrap().content.contains("5433"));

    let instruction = candidates
        .iter()
        .find(|c| c.category == MemoryType::Instruction);
    assert!(instruction.is_some(), "Instruction should be detected");
    assert!(instruction
        .unwrap()
        .content
        .to_lowercase()
        .contains("clippy"));
}

#[test]
fn test_observer_extracts_from_json_array() {
    let json_text = r#"[
        {"role": "user", "content": "Our API gateway endpoint is https://api.internal:8443."},
        {"role": "assistant", "content": "Noted API gateway endpoint."},
        {"role": "user", "content": "Our goal is to complete the migration by Friday."}
    ]"#;

    let candidates = ObserverService::extract(json_text, None, None, None, None, None)
        .expect("Extraction should succeed");

    assert_eq!(candidates.len(), 2);

    let fact = candidates.iter().find(|c| c.content.contains("8443"));
    assert!(fact.is_some(), "Fact with endpoint should be extracted");

    let goal = candidates.iter().find(|c| c.category == MemoryType::Goal);
    assert!(goal.is_some(), "Goal should be extracted");
}

#[test]
fn test_observer_saves_candidates_to_sqlite() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_observe.db");

    let cfg = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };

    let lm = LightMem::open_at(&db_path, cfg).expect("Failed to open controller");

    let transcript = "User: I always use zsh shell on macOS.\nAssistant: Understood.";
    let candidates = lm
        .observe_and_extract(transcript, None, Some(&["shell".to_string()]), None, None)
        .expect("Extraction should succeed");

    assert!(!candidates.is_empty());
    let candidate = &candidates[0];
    assert!(candidate.tags.contains(&"shell".to_string()));

    let saved = lm
        .save_observed_candidate(candidate)
        .expect("Should save candidate");
    assert_eq!(saved.provenance, "observed");
    assert_eq!(saved.category, MemoryType::Preference);

    // Verify it is searchable in SQLite
    let recalled = lm
        .recall("zsh shell", None, None, 5, None)
        .expect("Should recall");
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].memory.id, saved.id);
}
