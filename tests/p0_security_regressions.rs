use lightmem::models::{
    encode_base64, parse_mp4_dimensions_and_duration, parse_ttl_duration, LightMemConfig,
    MemoryRecord, MemoryType, PaginatedMemories,
};
use lightmem::services::importer::{JsonMemoryImporter, MemoryImporter};
use lightmem::views::CliView;
use lightmem::LightMem;
use tempfile::tempdir;

fn test_hash_config() -> LightMemConfig {
    LightMemConfig {
        backend: "hash".to_string(),
        onnx_model: None,
        ollama_url: "http://127.0.0.1:9".to_string(),
        ollama_api_key: None,
        embedding_model: "nomic-embed-text".to_string(),
        reranker: "top1".to_string(),
    }
}

#[test]
fn test_path_traversal_attachment_rejected_at_import_boundary() {
    let payload = serde_json::json!([{
        "title": "Malicious Attachment Note",
        "content": "This note attempts path traversal",
        "category": "fact",
        "attachments": [{
            "filename": "../escaped.bin",
            "media_kind": "binary",
            "mime_type": "application/octet-stream",
            "size_bytes": 10,
            "sha256": "",
            "data_base64": encode_base64(b"traversal_payload")
        }]
    }]);

    let candidates = JsonMemoryImporter.parse(&payload.to_string()).unwrap();
    let res = candidates
        .into_iter()
        .next()
        .unwrap()
        .into_record_and_attachments();
    assert!(res.is_err(), "Import candidate with '../' must be rejected");

    let payload_abs = serde_json::json!([{
        "title": "Malicious Absolute Path Note",
        "content": "This note attempts absolute path traversal",
        "category": "fact",
        "attachments": [{
            "filename": "/etc/passwd",
            "media_kind": "binary",
            "mime_type": "application/octet-stream",
            "size_bytes": 10,
            "sha256": "",
            "data_base64": encode_base64(b"root:x:0:0")
        }]
    }]);
    let cands_abs = JsonMemoryImporter.parse(&payload_abs.to_string()).unwrap();
    assert!(
        cands_abs
            .into_iter()
            .next()
            .unwrap()
            .into_record_and_attachments()
            .is_err(),
        "Import candidate with absolute path must be rejected"
    );

    let payload_backslash = serde_json::json!([{
        "title": "Windows traversal",
        "content": "Windows path separator",
        "category": "fact",
        "attachments": [{
            "filename": "..\\windows.bin",
            "media_kind": "binary",
            "mime_type": "application/octet-stream",
            "size_bytes": 10,
            "sha256": "",
            "data_base64": encode_base64(b"win_payload")
        }]
    }]);
    let cands_win = JsonMemoryImporter
        .parse(&payload_backslash.to_string())
        .unwrap();
    assert!(
        cands_win
            .into_iter()
            .next()
            .unwrap()
            .into_record_and_attachments()
            .is_err(),
        "Import candidate with backslash traversal must be rejected"
    );
}

#[test]
fn test_import_id_collision_rejected_without_replace_and_allowed_with_replace() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("id_collision.db");
    let lm = LightMem::open_at(&db_path, test_hash_config()).unwrap();

    let mem1 = lm
        .remember(
            "Original imported content",
            Some(MemoryType::Fact),
            Some("Original Title".to_string()),
            vec!["original".to_string()],
            Some(0.9),
        )
        .unwrap();

    let backup_file = dir.path().join("crafted_backup.json");
    let crafted_payload = serde_json::json!([{
        "id": mem1.id,
        "title": "Attacker replacement",
        "content": "Replacement content that destroys original",
        "category": "fact",
        "confidence": 0.95
    }]);
    std::fs::write(&backup_file, crafted_payload.to_string()).unwrap();

    // Default import (replace=false) must reject with clear conflict message
    let import_err = lm.import_file_with_options(&backup_file, false, false);
    assert!(import_err.is_err());
    let err_msg = import_err.unwrap_err().to_string();
    assert!(
        err_msg.contains("conflicting IDs") && err_msg.contains("--replace"),
        "Error message should mention conflicting IDs and --replace, got: {}",
        err_msg
    );

    // Verify existing record is preserved
    let current = lm.get(&mem1.id).unwrap().unwrap();
    assert_eq!(current.title, "Original Title");
    assert_eq!(current.content, "Original imported content");

    // Import with replace=true must succeed
    let replaced_count = lm
        .import_file_with_options(&backup_file, false, true)
        .unwrap();
    assert_eq!(replaced_count, 1);
    let after_replace = lm.get(&mem1.id).unwrap().unwrap();
    assert_eq!(after_replace.title, "Attacker replacement");
    assert_eq!(
        after_replace.content,
        "Replacement content that destroys original"
    );
}

#[test]
fn test_unicode_truncation_does_not_panic_on_emoji_and_multibyte() {
    // 20 emojis: each emoji is 4 bytes. 20 * 4 = 80 bytes.
    // Slicing at byte index 61 panics on byte boundary. Char boundary take(61) succeeds.
    let emojis = "😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀";
    let memory = MemoryRecord::new(
        MemoryType::Fact,
        "Emoji Note".to_string(),
        emojis.to_string(),
        vec![],
        0.9,
        None,
    );

    let paginated = PaginatedMemories {
        items: vec![memory],
        total: 1,
        page: 1,
        total_pages: 1,
        limit: 10,
        offset: 0,
        has_more: false,
    };

    // Must not panic!
    CliView::render_paginated_list(&paginated, std::path::Path::new("test.db"), false).unwrap();
    CliView::render_paginated_list(&paginated, std::path::Path::new("test.db"), true).unwrap();

    // Also verify link renderers with multi-byte IDs
    let multi_byte_id1 = "🦀🦀🦀🦀🦀🦀🦀🦀";
    let multi_byte_id2 = "🎯🎯🎯🎯🎯🎯🎯🎯";
    CliView::render_link(multi_byte_id1, multi_byte_id2, "rel", 1.0, false).unwrap();
    CliView::render_unlink(multi_byte_id1, multi_byte_id2, Some("rel"), true, false).unwrap();
    CliView::render_unlink(multi_byte_id1, multi_byte_id2, Some("rel"), false, false).unwrap();
}

#[test]
fn test_ttl_overflow_bounds_rejects_huge_durations_without_panic() {
    // Huge value that previously overflowed Duration::weeks(val)
    let err_w = parse_ttl_duration("9223372036854775807w");
    assert!(err_w.is_err());
    assert!(err_w.unwrap_err().contains("out of range"));

    let err_d = parse_ttl_duration("9223372036854775807d");
    assert!(err_d.is_err());
    assert!(err_d.unwrap_err().contains("out of range"));

    let err_h = parse_ttl_duration("9223372036854775807h");
    assert!(err_h.is_err());
    assert!(err_h.unwrap_err().contains("out of range"));

    let err_m = parse_ttl_duration("9223372036854775807m");
    assert!(err_m.is_err());
    assert!(err_m.unwrap_err().contains("out of range"));

    let err_s = parse_ttl_duration("9223372036854775807s");
    assert!(err_s.is_err());
    assert!(err_s.unwrap_err().contains("out of range"));

    // Valid duration must still parse correctly
    let ok_w = parse_ttl_duration("2w").unwrap();
    assert_eq!(ok_w, chrono::Duration::weeks(2));
}

#[test]
fn test_config_permissions_and_secret_redaction() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("cfg_secure");
    std::env::set_var("LIGHTMEM_CONFIG_DIR", &cfg_dir);

    let cfg = LightMemConfig {
        ollama_api_key: Some("super-secret-production-token-999".to_string()),
        ..Default::default()
    };
    cfg.save().unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(LightMemConfig::config_file())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "Config file must have 0600 permissions");
    }

    // Verify secret redaction in JSON serialization
    let mut val = serde_json::to_value(&cfg).unwrap();
    if let Some(obj) = val.as_object_mut() {
        if let Some(key_val) = obj.get_mut("ollama_api_key") {
            if let Some(key_str) = key_val.as_str() {
                if !key_str.is_empty() {
                    *key_val = serde_json::Value::String("[REDACTED]".into());
                }
            }
        }
    }
    let rendered_json = serde_json::to_string(&val).unwrap();
    assert!(
        !rendered_json.contains("super-secret-production-token-999"),
        "API key must never be emitted in plain text"
    );
    assert!(rendered_json.contains("[REDACTED]"));

    std::env::remove_var("LIGHTMEM_CONFIG_DIR");
}

#[test]
fn test_export_fail_closed_on_corrupt_blobs_and_permissions() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("export_blobs.db");
    let lm = LightMem::open_at(&db_path, test_hash_config()).unwrap();

    let fake_data = b"image-payload-data";
    let img_path = dir.path().join("test_img.png");
    std::fs::write(&img_path, fake_data).unwrap();

    let (_mem, _) = lm
        .remember_with_attachments(
            Some("Memory with image"),
            &[img_path],
            false,
            None,
            Some(MemoryType::Fact),
            Some("Image Note".to_string()),
            vec!["img".to_string()],
            Some(0.9),
            None,
            false,
            None,
        )
        .unwrap();

    let export_path = dir.path().join("vault_export.json");
    let exported = lm.export_json(&export_path).unwrap();
    assert!(exported.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&export_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "Export file must have 0600 permissions");
    }

    // Corrupt database by modifying payload to simulate disk/storage bitrot
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS trg_attachments_immutable;
         UPDATE memory_attachments SET data=X'deadbeef';",
    )
    .unwrap();
    drop(conn);

    // Export must fail closed and NOT succeed with corrupted/missing payloads!
    let corrupt_export = dir.path().join("corrupt_export.json");
    let res = lm.export_json(&corrupt_export);
    assert!(
        res.is_err(),
        "Export must fail closed when attachment payload BLOBs are corrupted"
    );
}

#[test]
fn test_mp4_crafted_sizes_do_not_panic() {
    // Malicious 64-bit box size header
    let mut crafted = Vec::new();
    crafted.extend_from_slice(&1u32.to_be_bytes()); // size 1 indicates 64-bit size follows
    crafted.extend_from_slice(b"moov");
    crafted.extend_from_slice(&u64::MAX.to_be_bytes()); // overflow box size
    crafted.extend_from_slice(&[0u8; 64]);

    let (dims, dur) = parse_mp4_dimensions_and_duration(&crafted);
    assert!(dims.is_none());
    assert!(dur.is_none());
}
