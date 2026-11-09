use lightmem::{LightMem, LightMemConfig, MemoryType};
use std::fs;
use tempfile::tempdir;

fn hash_config() -> LightMemConfig {
    LightMemConfig {
        backend: "hash".to_string(),
        onnx_model: None,
        ollama_url: "http://127.0.0.1:9".to_string(),
        ollama_api_key: None,
        embedding_model: "nomic-embed-text".to_string(),
        reranker: "top1".to_string(),
        ..Default::default()
    }
}

#[test]
fn test_attachment_deduplication_warning_and_skipping() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("dedup_vault.db");
    let lm = LightMem::open_at(&db_path, hash_config()).unwrap();

    // Create a dummy image
    let img_a = dir.path().join("diagram_a.png");
    fs::write(
        &img_a,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10\x00\x00\x00\x10\x08\x02\x00\x00\x00\x90\x91h6\x00\x00\x00\x0cIDATx\x9cc\xf8\xff\xff?\x00\x05\xfe\x02\xfe\xdc\xccY\xe7\x00\x00\x00\x00IEND\xaeB`\x82",
    )
    .unwrap();

    // Create an exact binary duplicate with a different filename
    let img_b = dir.path().join("diagram_b.png");
    fs::copy(&img_a, &img_b).unwrap();

    // 1. Remember first memory with img_a
    let (rec1, _, warn1) = lm
        .remember_with_attachments_and_entities(
            Some("First memory with diagram"),
            std::slice::from_ref(&img_a),
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            false,
        )
        .unwrap();

    assert!(
        warn1.is_none(),
        "First upload should have no duplicate warnings"
    );
    assert_eq!(rec1.attachments.len(), 1);

    // 2. Remember second memory with img_b (same SHA-256)
    let (_rec2, _, warn2) = lm
        .remember_with_attachments_and_entities(
            Some("Second memory referencing same diagram"),
            std::slice::from_ref(&img_b),
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            false,
        )
        .unwrap();

    assert!(
        warn2.is_some(),
        "Uploading existing SHA should emit a deduplication warning"
    );
    let w_str = warn2.unwrap();
    assert!(
        w_str.contains("already exists in vault") && w_str.contains("deduplicated"),
        "Warning was: {}",
        w_str
    );

    // 3. Intra-batch duplicates: passing img_a and img_b in the same call
    let img_c = dir.path().join("diagram_c.png");
    fs::copy(&img_a, &img_c).unwrap();
    let (rec3, _, warn3) = lm
        .remember_with_attachments_and_entities(
            Some("Third memory with batch duplicates"),
            &[img_a, img_c],
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            false,
        )
        .unwrap();

    assert_eq!(
        rec3.attachments.len(),
        1,
        "Redundant blob in same batch must be skipped"
    );
    assert!(warn3.is_some());
    let w3_str = warn3.unwrap();
    assert!(w3_str.contains("duplicate in batch") && w3_str.contains("skipping redundant blob"));
}

#[test]
fn test_local_only_memory_flag_and_isolation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("local_vault.db");
    let lm = LightMem::open_at(&db_path, hash_config()).unwrap();

    let doc = dir.path().join("confidential.txt");
    fs::write(&doc, b"Confidential tax filings").unwrap();

    let (rec, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Confidential financial statement"),
            &[doc],
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec!["finance".to_string()],
            Some(0.9),
            None,
            false,
            None,
            true, // local = true
        )
        .unwrap();

    assert!(rec.is_local(), "Memory must report is_local() == true");
    assert!(
        rec.tags.contains(&"local".to_string()),
        "Must have auto-applied #local tag"
    );
    assert_eq!(rec.provenance, "local");
}

#[test]
fn test_cas_cloud_sync_and_vector_snapshots() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("main_vault.db");
    let sync_target = dir.path().join("CloudDrive").join("LightMem");

    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    let lm = LightMem::open_at(&db_path, cfg.clone()).unwrap();

    // 1. Add normal syncable memory with an attachment
    let img_sync = dir.path().join("photo.png");
    fs::write(
        &img_sync,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x08\x00\x00\x00\x08\x08\x02\x00\x00\x00\x4b\x6d\x9c\x35\x00\x00\x00\x0cIDATx\x9cc\xf8\xff\xff?\x00\x05\xfe\x02\xfe\xdc\xccY\xe7\x00\x00\x00\x00IEND\xaeB`\x82",
    )
    .unwrap();

    let (rec_sync, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Public presentation slide"),
            &[img_sync],
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec!["work".to_string()],
            Some(0.9),
            None,
            false,
            None,
            false, // not local
        )
        .unwrap();

    // 2. Add local-only memory with an attachment
    let img_local = dir.path().join("private_passport.png");
    fs::write(
        &img_local,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x04\x00\x00\x00\x04\x08\x02\x00\x00\x00\x26\x93\x0f\x12\x00\x00\x00\x0cIDATx\x9cc\xf8\xff\xff?\x00\x05\xfe\x02\xfe\xdc\xccY\xe7\x00\x00\x00\x00IEND\xaeB`\x82",
    )
    .unwrap();

    let (rec_local, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Private passport scan"),
            &[img_local],
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            true, // local = true
        )
        .unwrap();

    // 3. Perform push sync
    let report = lm.sync_push(None, false).unwrap();
    assert_eq!(
        report.blobs_written, 1,
        "Only 1 non-local blob should be copied to CAS"
    );
    assert_eq!(
        report.local_only_memories, 1,
        "1 local memory must be exempt"
    );
    assert_eq!(report.total_active_memories, 1, "1 memory synced");

    let blobs_dir = sync_target.join("blobs");
    let snapshots_dir = sync_target.join("snapshots");
    assert!(blobs_dir.exists());
    assert!(snapshots_dir.exists());

    // Verify non-local blob exists in CAS
    let sha_sync = &rec_sync.attachments[0].sha256;
    let expected_blob = blobs_dir.join(format!("{}.png", sha_sync));
    assert!(expected_blob.exists(), "Synced blob must exist in CAS");

    // Verify local blob DOES NOT exist in CAS
    let sha_local = &rec_local.attachments[0].sha256;
    let forbidden_blob = blobs_dir.join(format!("{}.png", sha_local));
    assert!(
        !forbidden_blob.exists(),
        "Local-only blob must NEVER be written to CAS target"
    );

    // Verify vector snapshot was created
    let latest_db = snapshots_dir.join("vault_latest.db");
    assert!(latest_db.exists(), "vault_latest.db snapshot must exist");
    assert!(report.snapshot_bytes > 0);

    // 4. Test Deduplication on secondary sync (0 blobs re-written)
    let report2 = lm.sync_push(None, false).unwrap();
    assert_eq!(
        report2.blobs_written, 0,
        "Already-synced blobs must not be re-written (CAS)"
    );
    assert_eq!(report2.blobs_skipped_existing, 1, "1 blob skipped existing");

    // 5. Test Status
    let status = lm.sync_status(None).unwrap();
    assert!(status.configured);
    assert_eq!(status.blobs_count, 1);
    assert_eq!(status.local_only_memories_count, 1);
    assert_eq!(status.syncable_memories_count, 1);

    // 6. Test Pull & Restore into a fresh database
    let restored_db_path = dir.path().join("restored_vault.db");
    let restored_lm = LightMem::open_at(&restored_db_path, cfg).unwrap();
    let pull_report = restored_lm.sync_pull(Some(&sync_target), true).unwrap();
    assert!(pull_report.restored_db);
    assert_eq!(pull_report.blobs_found_in_target, 1);

    // The restored vault should have the synced memory
    let fetched = restored_lm.get(&rec_sync.id).unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().title, "Public presentation slide");
}

#[test]
fn test_gdrive_auth_status_and_logout() {
    let temp = tempfile::tempdir().unwrap();
    let home_path = temp.path().to_path_buf();
    std::env::set_var("LIGHTMEM_HOME", &home_path);

    // 1. Initial state: disconnected
    assert!(!lightmem::GoogleDriveClient::is_connected());
    let status_initial = lightmem::GoogleDriveClient::get_status().unwrap();
    assert!(!status_initial.connected);
    assert!(!status_initial.token_present);
    assert!(!status_initial.credentials_present);

    // 2. Setup mock credentials and tokens
    let creds_file = lightmem::GoogleDriveClient::credentials_path();
    let token_file = lightmem::GoogleDriveClient::token_path();
    let folders_file = lightmem::GoogleDriveClient::folders_path();

    std::fs::write(
        &creds_file,
        r#"{"installed":{"client_id":"mock-id","client_secret":"mock-secret"}}"#,
    )
    .unwrap();
    std::fs::write(
        &token_file,
        r#"{"access_token":"ya29.mock","refresh_token":"1//mock","user_email":"testuser@example.com","scope":"https://www.googleapis.com/auth/drive.file"}"#,
    )
    .unwrap();
    std::fs::write(
        &folders_file,
        r#"{"root_id":"root123","blobs_id":"blobs456","snapshots_id":"snap789"}"#,
    )
    .unwrap();

    assert!(lightmem::GoogleDriveClient::is_connected());
    let status = lightmem::GoogleDriveClient::get_status().unwrap();
    assert!(status.connected);
    assert_eq!(status.user_email.as_deref(), Some("testuser@example.com"));
    assert!(status.token_present);
    assert!(status.credentials_present);
    assert!(status.folders.is_some());
    assert_eq!(status.folders.unwrap().blobs_id, "blobs456");

    // 3. Test logout without purging credentials (preserves credentials file, deletes tokens & folders)
    let logout_rep = lightmem::GoogleDriveClient::logout(false, false).unwrap();
    assert_eq!(
        logout_rep.user_email.as_deref(),
        Some("testuser@example.com")
    );
    assert!(logout_rep.token_removed);
    assert!(logout_rep.folders_cache_removed);
    assert!(!logout_rep.credentials_removed);
    assert!(!token_file.exists());
    assert!(!folders_file.exists());
    assert!(creds_file.exists());
    assert!(!lightmem::GoogleDriveClient::is_connected());

    // 4. Test logout with purge_credentials = true
    let logout_rep2 = lightmem::GoogleDriveClient::logout(false, true).unwrap();
    assert!(logout_rep2.credentials_removed);
    assert!(!creds_file.exists());

    std::env::remove_var("LIGHTMEM_HOME");
}

#[test]
fn test_slim_snapshot_dehydrates_cas_blobs_and_rehydrates_on_restore() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("source_vault.db");
    let sync_target = dir.path().join("CloudSync");

    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    let lm = LightMem::open_at(&db_path, cfg.clone()).unwrap();

    // Large synthetic attachment (~600 KB) + 0-byte attachment + local-only attachment
    let large_payload: Vec<u8> = (0..600_000).map(|i| ((i * 73 + 19) % 251) as u8).collect();
    let large_file = dir.path().join("clip.mp4");
    fs::write(&large_file, &large_payload).unwrap();

    let empty_file = dir.path().join("empty_marker.txt");
    fs::write(&empty_file, b"").unwrap();

    let local_payload = vec![0xAAu8; 40_000];
    let local_file = dir.path().join("local_secret.bin");
    fs::write(&local_file, &local_payload).unwrap();

    let (rec_media, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Recorded 4K demo session with NeonVector graph"),
            &[large_file, empty_file],
            false,
            false,
            None,
            Some(MemoryType::Artifact),
            Some("Demo Session".to_string()),
            vec!["video".to_string(), "demo".to_string()],
            Some(0.95),
            None,
            false,
            None,
            false,
        )
        .unwrap();

    let rec_note = lm
        .remember(
            "Connected architecture note referencing [[Demo Session]]",
            Some(MemoryType::Decision),
            Some("Architecture Note".to_string()),
            vec!["arch".to_string()],
            Some(0.9),
        )
        .unwrap();
    lm.link(&rec_note.id, &rec_media.id, Some("documents"), Some(1.5))
        .unwrap();

    let (rec_local, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Local offline key material"),
            &[local_file],
            false,
            false,
            None,
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            true,
        )
        .unwrap();

    // 1. Push normal slim CAS sync
    let report1 = lm.sync_push(None, false).unwrap();
    assert_eq!(report1.blobs_written, 2); // clip.mp4 + empty_marker.txt
    assert_eq!(report1.local_only_memories, 1);

    let latest_db = sync_target.join("snapshots").join("vault_latest.db");
    assert!(latest_db.exists());
    assert!(
        report1.snapshot_bytes < 250_000,
        "Slim CAS snapshot must dehydrate the 600 KB video BLOB; got {} bytes",
        report1.snapshot_bytes
    );

    // Inspect vault_latest.db directly: synced video payload must be empty x'', local payload retained
    {
        let conn = rusqlite::Connection::open(&latest_db).unwrap();
        let (dehydrated_len, stored_size): (i64, i64) = conn
            .query_row(
                "SELECT length(data), size_bytes FROM memory_attachments WHERE memory_id = ?1 AND filename = 'clip.mp4'",
                [&rec_media.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dehydrated_len, 0);
        assert_eq!(stored_size, large_payload.len() as i64);

        let local_len: i64 = conn
            .query_row(
                "SELECT length(data) FROM memory_attachments WHERE memory_id = ?1",
                [&rec_local.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(local_len, local_payload.len() as i64);
    }

    // 2. Second push skips reading & uploading existing blobs lazily
    let report2 = lm.sync_push(None, false).unwrap();
    assert_eq!(report2.blobs_written, 0);
    assert_eq!(report2.blobs_skipped_existing, 2);
    assert_eq!(report2.blob_bytes_transferred, 0);

    // 3. Restore into fresh vault & verify byte-for-byte rehydration + metadata + FTS5 + graph + immutability trigger
    let restored_path = dir.path().join("restored.db");
    let restored_lm = LightMem::open_at(&restored_path, cfg).unwrap();
    let pull_rep = restored_lm.sync_pull(Some(&sync_target), true).unwrap();
    assert!(pull_rep.restored_db);

    let att_data = restored_lm
        .storage()
        .get_memory_attachments_data(&rec_media.id)
        .unwrap();
    assert_eq!(att_data.len(), 2);
    let clip_entry = att_data
        .iter()
        .find(|(m, _)| m.filename == "clip.mp4")
        .unwrap();
    assert_eq!(clip_entry.1, large_payload);
    let empty_entry = att_data
        .iter()
        .find(|(m, _)| m.filename == "empty_marker.txt")
        .unwrap();
    assert!(empty_entry.1.is_empty());

    // Graph links (auto-wikilink + explicit link) & FTS search intact on restored vault
    let links = restored_lm
        .storage()
        .get_links_for_memory(&rec_note.id)
        .unwrap();
    assert_eq!(links.len(), 2);
    assert!(links
        .iter()
        .any(|l| l.target_id == rec_media.id && l.relation == "references"));
    assert!(links
        .iter()
        .any(|l| l.target_id == rec_media.id && l.relation == "documents"));
    let fts_hits = restored_lm
        .storage()
        .search_bm25(
            "clip.mp4",
            None,
            Some(lightmem::MemoryStatus::Active),
            None,
            5,
        )
        .unwrap();
    assert!(fts_hits.iter().any(|(id, _)| id == &rec_media.id));

    // Hybrid vector + FTS5 recall intact on restored vault without re-embedding
    let recall_hits = restored_lm
        .recall("Recorded 4K demo session", None, None, 5, None)
        .unwrap();
    assert!(
        recall_hits.iter().any(|r| r.memory.id == rec_media.id),
        "Restored vault must preserve vectors and recall synced memories"
    );

    // Rehydration must leave trg_attachments_immutable enforced
    {
        let conn = rusqlite::Connection::open(&restored_path).unwrap();
        let update_res = conn.execute(
            "UPDATE memory_attachments SET data = x'00' WHERE memory_id = ?1",
            [&rec_media.id],
        );
        assert!(
            update_res.is_err(),
            "Immutability trigger must still block direct UPDATE after CAS rehydration"
        );
    }
}

#[test]
fn test_backup_only_snapshot_stays_self_contained_and_restores_without_blobs() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("backup_only_src.db");
    let sync_target = dir.path().join("BackupOnlyTarget");

    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    let lm = LightMem::open_at(&db_path, cfg.clone()).unwrap();

    let payload = vec![0x5Au8; 180_000];
    let file = dir.path().join("recording.mp4");
    fs::write(&file, &payload).unwrap();

    let (rec, _, _) = lm
        .remember_with_attachments_and_entities(
            Some("Self-contained DB backup check"),
            &[file],
            false,
            false,
            None,
            Some(MemoryType::Artifact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            false,
        )
        .unwrap();

    let report = lm.sync_push(None, true).unwrap();
    assert_eq!(report.blobs_written, 0);
    assert!(
        report.snapshot_bytes >= payload.len() as u64,
        "--backup-only snapshot must retain full attachment BLOBs"
    );

    // Delete empty blobs directory and verify full restore still works
    let _ = fs::remove_dir_all(sync_target.join("blobs"));
    let restored_path = dir.path().join("restored_backup_only.db");
    let restored = LightMem::open_at(&restored_path, cfg).unwrap();
    let pull = restored.sync_pull(Some(&sync_target), true).unwrap();
    assert!(pull.restored_db);
    let items = restored
        .storage()
        .get_memory_attachments_data(&rec.id)
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1, payload);
}

#[test]
fn test_restore_aborts_atomically_on_missing_or_corrupt_cas_blob() {
    let dir = tempdir().unwrap();
    let src_db = dir.path().join("src.db");
    let sync_target = dir.path().join("CAS");

    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    let src = LightMem::open_at(&src_db, cfg.clone()).unwrap();

    let payload = vec![0x7Cu8; 50_000];
    let file = dir.path().join("asset.bin");
    fs::write(&file, &payload).unwrap();
    let (rec, _, _) = src
        .remember_with_attachments_and_entities(
            Some("Important remote asset"),
            &[file],
            false,
            false,
            None,
            Some(MemoryType::Artifact),
            None,
            vec![],
            Some(0.9),
            None,
            false,
            None,
            false,
        )
        .unwrap();
    src.sync_push(None, false).unwrap();

    let blob_path = sync_target
        .join("blobs")
        .join(format!("{}.bin", rec.attachments[0].sha256));
    assert!(blob_path.exists());

    // Target vault already has valuable local memory
    let dst_db = dir.path().join("dst.db");
    let dst = LightMem::open_at(&dst_db, cfg).unwrap();
    let keep_rec = dst
        .remember(
            "Pre-existing vault memory that must not be lost",
            Some(MemoryType::Fact),
            Some("Keep Me Safe".to_string()),
            vec![],
            Some(1.0),
        )
        .unwrap();

    // Case 1: Corrupt blob in CAS -> restore must fail & keep dst intact
    let mut corrupted = payload.clone();
    corrupted[1234] ^= 0xFF;
    fs::write(&blob_path, &corrupted).unwrap();
    let corrupt_err = dst.sync_pull(Some(&sync_target), true).unwrap_err();
    assert!(
        corrupt_err.to_string().contains("SHA-256 mismatch"),
        "Unexpected error for corrupt blob: {}",
        corrupt_err
    );
    assert!(
        dst.get(&keep_rec.id).unwrap().is_some(),
        "Existing vault must stay intact if rehydration fails"
    );

    // Case 2: Missing blob in CAS -> restore must fail & keep dst intact
    fs::remove_file(&blob_path).unwrap();
    let missing_err = dst.sync_pull(Some(&sync_target), true).unwrap_err();
    assert!(
        missing_err.to_string().contains("Missing CAS blob"),
        "Unexpected error for missing blob: {}",
        missing_err
    );
    assert!(
        dst.get(&keep_rec.id).unwrap().is_some(),
        "Existing vault must stay intact when CAS blob is missing"
    );
}
