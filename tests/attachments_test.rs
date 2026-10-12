use lightmem::{compute_sha256_hex, LightMem, LightMemConfig, MemoryType};
use rusqlite::Connection;
use std::io::{Read, Write};
use std::net::TcpListener;
use tempfile::tempdir;

fn make_fake_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    // PNG magic signature
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // IHDR length (13) + "IHDR"
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    // bit depth, color type, compression, filter, interlace + dummy CRC
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn make_fake_wav(byte_rate: u32, data_bytes: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    let total_size = 36 + data_bytes;
    bytes.extend_from_slice(&total_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    // fmt chunk (16 bytes)
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // 1 channel
    bytes.extend_from_slice(&16000u32.to_le_bytes()); // sample rate
    bytes.extend_from_slice(&byte_rate.to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
                                                   // data chunk
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.extend(std::iter::repeat_n(0x55u8, data_bytes as usize));
    bytes
}

fn hash_config() -> LightMemConfig {
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
fn test_attachment_blob_storage_metadata_search_immutability_and_delete() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("vault.db");
    let lm = LightMem::open_at(&db_path, hash_config()).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "SQLite database file must be owner-only (0600)"
        );
    }

    // 1. Create a PNG image (1920x1080) and a WAV audio file (2.5 seconds)
    let png_path = dir.path().join("architecture_diagram.png");
    let png_bytes = make_fake_png(1920, 1080);
    std::fs::write(&png_path, &png_bytes).unwrap();

    let wav_path = dir.path().join("standup_voice_note.wav");
    // byte_rate = 32000, data_bytes = 80000 -> 2.5 seconds
    let wav_bytes = make_fake_wav(32000, 80000);
    std::fs::write(&wav_path, &wav_bytes).unwrap();

    // 2. Store image attachment with metadata-only default (extract_llm = false)
    let (img_mem, _) = lm
        .remember_with_attachments(
            Some("System topology diagram for Kubernetes ingress"),
            std::slice::from_ref(&png_path),
            false,
            None,
            Some(MemoryType::Artifact),
            Some("K8s Ingress Diagram".to_string()),
            vec!["k8s".to_string()],
            Some(0.95),
            None,
            false,
            None,
        )
        .unwrap();

    assert_eq!(img_mem.attachments.len(), 1);
    let img_att = &img_mem.attachments[0];
    assert_eq!(img_att.filename, "architecture_diagram.png");
    assert_eq!(img_att.mime_type, "image/png");
    assert_eq!(img_att.media_kind, "image");
    assert_eq!(img_att.dimensions.as_deref(), Some("1920x1080"));
    assert_eq!(img_att.size_bytes, png_bytes.len() as u64);
    assert_eq!(img_att.sha256, compute_sha256_hex(&png_bytes));
    assert!(img_mem.tags.contains(&"image".to_string()));
    assert!(
        img_mem.to_card_text().contains("architecture_diagram.png")
            && img_mem.to_card_text().contains("1920x1080"),
        "Memory card text (used for vector embedding) must include attachment metadata"
    );

    // 3. Store audio attachment without text caption (metadata auto-caption)
    let (aud_mem, _) = lm
        .remember_with_attachments(
            None,
            std::slice::from_ref(&wav_path),
            false,
            None,
            None,
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    assert_eq!(aud_mem.category, MemoryType::Artifact);
    assert_eq!(aud_mem.attachments.len(), 1);
    let aud_att = &aud_mem.attachments[0];
    assert_eq!(aud_att.filename, "standup_voice_note.wav");
    assert_eq!(aud_att.mime_type, "audio/wav");
    assert_eq!(aud_att.media_kind, "audio");
    assert!((aud_att.duration_secs.unwrap() - 2.5).abs() < 0.05);

    // 4. Query by attachment metadata (filename, dimensions, sha256) via hybrid recall
    let by_filename = lm
        .recall("architecture_diagram.png", None, None, 5, None)
        .unwrap();
    assert!(!by_filename.is_empty());
    assert_eq!(by_filename[0].memory.id, img_mem.id);
    assert_eq!(by_filename[0].memory.attachments.len(), 1);

    let by_dims = lm.recall("1920x1080", None, None, 5, None).unwrap();
    assert!(!by_dims.is_empty());
    assert_eq!(by_dims[0].memory.id, img_mem.id);

    let by_audio = lm
        .recall("standup_voice_note", None, None, 5, None)
        .unwrap();
    assert!(!by_audio.is_empty());
    assert_eq!(by_audio[0].memory.id, aud_mem.id);

    // 5. Verify SQLite immutability trigger prevents in-place modification of attachment rows
    {
        let raw_conn = Connection::open(&db_path).unwrap();
        let update_data_err = raw_conn.execute(
            "UPDATE memory_attachments SET data = X'00' WHERE memory_id = ?1",
            [&img_mem.id],
        );
        assert!(
            update_data_err.is_err(),
            "SQLite trigger trg_attachments_immutable must block UPDATE on attachment data"
        );
        let update_name_err = raw_conn.execute(
            "UPDATE memory_attachments SET filename = 'hacked.png' WHERE memory_id = ?1",
            [&img_mem.id],
        );
        assert!(
            update_name_err.is_err(),
            "SQLite trigger trg_attachments_immutable must block UPDATE on any attachment column"
        );
    }

    // 6. Export attachment back to disk via save_attachments_to and verify exact bytes & SHA-256
    let export_dir = dir.path().join("exported");
    let saved_files = lm.save_attachments_to(&img_mem.id, &export_dir).unwrap();
    assert_eq!(saved_files.len(), 1);
    let exported_bytes = std::fs::read(&saved_files[0]).unwrap();
    assert_eq!(exported_bytes, png_bytes);

    // 7. Hard-deleting the memory cascades and removes the attachment BLOB from memory_attachments
    assert!(lm.forget(&img_mem.id, true).unwrap());
    {
        let raw_conn = Connection::open(&db_path).unwrap();
        let remaining: i64 = raw_conn
            .query_row(
                "SELECT COUNT(*) FROM memory_attachments WHERE memory_id = ?1",
                [&img_mem.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            remaining, 0,
            "Hard-deleting a memory must cascade-delete its attachments"
        );
    }
}

#[test]
fn test_explicit_single_model_llm_extraction_on_attachment() {
    // Spawn a minimal single-request mock OpenAI-compatible /v1/chat/completions server
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"choices":[{"message":{"content":"Extracted diagram shows API Gateway routing to PostgreSQL 16 on port 5432."}}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            req_str
        } else {
            String::new()
        }
    });

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("llm_extract.db");
    let cfg = LightMemConfig {
        backend: "hash".to_string(),
        onnx_model: None,
        ollama_url: format!("http://{}", addr),
        ollama_api_key: None,
        embedding_model: "nomic-embed-text".to_string(),
        reranker: "ollama:gemma4:31b-cloud".to_string(),
    };
    let lm = LightMem::open_at(&db_path, cfg).unwrap();

    let png_path = dir.path().join("flow.png");
    std::fs::write(&png_path, make_fake_png(800, 600)).unwrap();

    let (mem, _) = lm
        .remember_with_attachments(
            Some("Production database flow"),
            std::slice::from_ref(&png_path),
            true, // Explicit --extract
            None,
            None,
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();

    let captured_req = server_handle.join().unwrap();
    assert!(
        captured_req.contains("/v1/chat/completions"),
        "Must call single LLM via /v1/chat/completions"
    );
    assert!(
        captured_req.contains("gemma4:31b-cloud"),
        "Must use the single configured main LLM model"
    );
    assert!(
        mem.content
            .contains("Extracted diagram shows API Gateway routing to PostgreSQL 16 on port 5432."),
        "Memory content must include LLM-extracted description when extract_llm is true"
    );
    assert_eq!(mem.provenance, "attachment_llm_extracted");
    assert_eq!(mem.attachments.len(), 1);
}

#[test]
fn test_cli_remember_file_recall_media_inspect_save_to_and_forget() {
    use std::process::Command;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_vault.db");
    // Pre-initialize DB with hash backend via config in a temporary HOME
    let home_dir = dir.path().join("home");
    std::fs::create_dir_all(home_dir.join(".lightmem")).unwrap();
    std::fs::write(
        home_dir.join(".lightmem/config.json"),
        serde_json::to_string(&hash_config()).unwrap(),
    )
    .unwrap();

    let png_path = dir.path().join("schema_v4.png");
    let png_bytes = make_fake_png(1280, 720);
    std::fs::write(&png_path, &png_bytes).unwrap();

    let bin = env!("CARGO_BIN_EXE_lmem");

    // 1. lmem remember -f schema_v4.png --json
    let out_rem = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "Database schema v4 diagram",
            "-f",
            png_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out_rem.status.success(),
        "lmem remember -f failed: {}",
        String::from_utf8_lossy(&out_rem.stderr)
    );
    let rem_json: serde_json::Value = serde_json::from_slice(&out_rem.stdout).unwrap();
    let mem_id = rem_json["id"].as_str().unwrap().to_string();
    assert_eq!(rem_json["attachments"][0]["filename"], "schema_v4.png");
    assert_eq!(rem_json["attachments"][0]["dimensions"], "1280x720");

    // 2. lmem recall "1280x720" --media image --json
    let out_rec = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "recall",
            "1280x720",
            "--media",
            "image",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out_rec.status.success());
    let rec_json: serde_json::Value = serde_json::from_slice(&out_rec.stdout).unwrap();
    assert_eq!(rec_json.as_array().unwrap().len(), 1);
    assert_eq!(rec_json[0]["memory"]["id"], mem_id);

    // 3. lmem inspect <id> --save-to <restored.png> --json
    let restored_path = dir.path().join("restored_schema.png");
    let out_insp = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "inspect",
            &mem_id,
            "--save-to",
            restored_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out_insp.status.success());
    assert_eq!(std::fs::read(&restored_path).unwrap(), png_bytes);

    // 3b. Store a memory with MULTIPLE attachments (-f png -f wav) and verify --paths outputs both paths
    let wav_path = dir.path().join("theme_audio.wav");
    let wav_bytes = make_fake_wav(32000, 32000);
    std::fs::write(&wav_path, &wav_bytes).unwrap();

    let out_multi = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "Multi-asset bundle",
            "-f",
            png_path.to_str().unwrap(),
            "-f",
            wav_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out_multi.status.success());
    let multi_json: serde_json::Value = serde_json::from_slice(&out_multi.stdout).unwrap();
    let multi_id = multi_json["id"].as_str().unwrap().to_string();
    assert_eq!(multi_json["attachments"].as_array().unwrap().len(), 2);

    // lmem inspect <multi_id> --paths (should print 2 verified paths, one per line)
    let out_paths = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "inspect",
            &multi_id,
            "--paths",
        ])
        .output()
        .unwrap();
    assert!(out_paths.status.success());
    let stdout_paths: Vec<&str> = std::str::from_utf8(&out_paths.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(
        stdout_paths.len(),
        2,
        "inspect --paths on a 2-attachment memory must print 2 paths"
    );
    assert!(stdout_paths[0].ends_with("schema_v4.png"));
    assert!(stdout_paths[1].ends_with("theme_audio.wav"));
    assert_eq!(std::fs::read(stdout_paths[0]).unwrap(), png_bytes);
    assert_eq!(std::fs::read(stdout_paths[1]).unwrap(), wav_bytes);

    // lmem recall "Multi-asset bundle" -l 1 --paths
    let out_rec_paths = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "recall",
            "Multi-asset bundle",
            "-l",
            "1",
            "--paths",
        ])
        .output()
        .unwrap();
    assert!(out_rec_paths.status.success());
    let rec_lines: Vec<&str> = std::str::from_utf8(&out_rec_paths.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(rec_lines.len(), 2);

    // 4. lmem forget <id> --hard --json
    let out_fgt = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "forget",
            &mem_id,
            "--hard",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out_fgt.status.success());
}

#[test]
fn test_qa_regressions_attachments_webp_dedup_export_conflict_and_cli() {
    use lightmem::models::AttachmentInput;
    use std::process::Command;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("qa_regressions.db");
    let home_dir = dir.path().join("home");
    std::fs::create_dir_all(&home_dir).unwrap();
    let bin = env!("CARGO_BIN_EXE_lmem");

    // 1. WebP dimensions (VP8X, VP8, VP8L)
    let mut vp8x = Vec::new();
    vp8x.extend_from_slice(b"RIFF\x24\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00");
    vp8x.extend_from_slice(&(499u32).to_le_bytes()[..3]); // width = 500
    vp8x.extend_from_slice(&(599u32).to_le_bytes()[..3]); // height = 600
    vp8x.extend_from_slice(&[0u8; 16]);
    let att_vp8x = AttachmentInput::from_bytes("banner.webp", vp8x);
    assert_eq!(att_vp8x.metadata.media_kind, "image");
    assert_eq!(att_vp8x.metadata.dimensions.as_deref(), Some("500x600"));

    // 2. Duplicate -f in same remember command does not inflate title/content
    let png1_path = dir.path().join("one.png");
    let png2_path = dir.path().join("two.png");
    std::fs::write(&png1_path, make_fake_png(100, 200)).unwrap();
    std::fs::write(&png2_path, make_fake_png(300, 400)).unwrap();

    let out_dup = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "-f",
            png1_path.to_str().unwrap(),
            "-f",
            png1_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out_dup.status.success());
    let dup_json: serde_json::Value = serde_json::from_slice(&out_dup.stdout).unwrap();
    assert_eq!(dup_json["title"], "one.png");
    assert_eq!(dup_json["attachments"].as_array().unwrap().len(), 1);
    assert!(!dup_json["content"].as_str().unwrap().contains("+1 more"));

    // 3. Multi-memory recall --save-to single file path disambiguates filenames & returns saved_attachments
    let r1_out = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "Shared keyword alpha one",
            "-f",
            png1_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let r1_json: serde_json::Value = serde_json::from_slice(&r1_out.stdout).unwrap();
    let id1 = r1_json["id"].as_str().unwrap().to_string();

    let r2_out = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "Shared keyword alpha two",
            "-f",
            png2_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let r2_json: serde_json::Value = serde_json::from_slice(&r2_out.stdout).unwrap();
    let id2 = r2_json["id"].as_str().unwrap().to_string();

    let single_target = dir.path().join("single_out.png");
    let rec_save = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "recall",
            "Shared keyword alpha",
            "-l",
            "2",
            "--save-to",
            single_target.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(rec_save.status.success());
    let rec_arr: Vec<serde_json::Value> = serde_json::from_slice(&rec_save.stdout).unwrap();
    assert_eq!(rec_arr.len(), 2);
    let saved_0 = rec_arr[0]["saved_attachments"][0]
        .as_str()
        .unwrap()
        .to_string();
    let saved_1 = rec_arr[1]["saved_attachments"][0]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(
        saved_0, saved_1,
        "Single-file --save-to across multiple memories must not overwrite the first file"
    );
    assert!(std::path::Path::new(&saved_0).exists());
    assert!(std::path::Path::new(&saved_1).exists());

    // 4. Inspect single vs multi-ID JSON schema parity & inspect --path on plain memory
    let plain_out = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "remember",
            "Plain memory without files on 2026-04-01",
            "--json",
        ])
        .output()
        .unwrap();
    let plain_json: serde_json::Value = serde_json::from_slice(&plain_out.stdout).unwrap();
    let plain_id = plain_json["id"].as_str().unwrap().to_string();

    let insp_single: serde_json::Value = serde_json::from_slice(
        &Command::new(bin)
            .env("HOME", &home_dir)
            .args(["--db", db_path.to_str().unwrap(), "inspect", &id1, "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let insp_multi: Vec<serde_json::Value> = serde_json::from_slice(
        &Command::new(bin)
            .env("HOME", &home_dir)
            .args([
                "--db",
                db_path.to_str().unwrap(),
                "inspect",
                &id1,
                &id2,
                "--json",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let single_keys: Vec<&String> = insp_single.as_object().unwrap().keys().collect();
    let multi_keys: Vec<&String> = insp_multi[0].as_object().unwrap().keys().collect();
    assert_eq!(single_keys, multi_keys);

    let insp_plain_path = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "inspect",
            &plain_id,
            "--path",
        ])
        .output()
        .unwrap();
    assert!(
        !insp_plain_path.status.success(),
        "inspect --path on memory with 0 attachments must exit non-zero"
    );

    // 5. Export JSON & Import roundtrip preserves binary attachments
    let export_json_path = dir.path().join("vault_export.json");
    let exp_out = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "export",
            "--json",
            "-o",
            export_json_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(exp_out.status.success());

    let imported_db = dir.path().join("imported.db");
    let imp_out = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            imported_db.to_str().unwrap(),
            "import",
            export_json_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(imp_out.status.success());

    let imp_insp_path = Command::new(bin)
        .env("HOME", &home_dir)
        .args([
            "--db",
            imported_db.to_str().unwrap(),
            "inspect",
            &id1,
            "--path",
        ])
        .output()
        .unwrap();
    assert!(imp_insp_path.status.success());
    let materialized = std::str::from_utf8(&imp_insp_path.stdout).unwrap().trim();
    assert_eq!(
        std::fs::read(materialized).unwrap(),
        make_fake_png(100, 200)
    );

    // 6. Conflict merge (--supersede) preserves older memory's attachments on survivor
    let conflict_db = dir.path().join("conflict.db");
    let _ = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            conflict_db.to_str().unwrap(),
            "remember",
            "Production API gateway runs on port 8080",
            "--tags",
            "api,gateway,port",
            "-f",
            png1_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let sup_out = Command::new(bin)
        .env("HOME", &home_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .args([
            "--db",
            conflict_db.to_str().unwrap(),
            "remember",
            "Production API gateway does not run on port 8080, migrated to port 9090",
            "--tags",
            "api,gateway,port",
            "--supersede",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(sup_out.status.success());
    let sup_json: serde_json::Value = serde_json::from_slice(&sup_out.stdout).unwrap();
    assert_eq!(
        sup_json["attachments"].as_array().unwrap().len(),
        1,
        "Superseded survivor must retain older memory's attachment"
    );
}
