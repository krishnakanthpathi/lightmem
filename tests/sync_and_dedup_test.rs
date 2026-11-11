use lightmem::{LightMem, LightMemConfig, MemoryRecord, MemoryStatus, MemoryType, Storage};
use std::fs;
use std::path::Path;
use std::process::Command;
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

fn extract_memories_db_from_zip(zip_path: &Path, dest_db_path: &Path) {
    let file = fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut found_idx = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).unwrap();
        if entry
            .name()
            .map(|n| n.as_ref() == "memories.db")
            .unwrap_or(false)
        {
            found_idx = Some(i);
            break;
        }
    }
    let idx = found_idx.expect("vault_latest.db.zip must contain memories.db entry");
    let mut entry = archive.by_index(idx).unwrap();
    let mut out = fs::File::create(dest_db_path).unwrap();
    std::io::copy(&mut entry, &mut out).unwrap();
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

    // Verify compressed vector snapshot was created
    let latest_zip = snapshots_dir.join("vault_latest.db.zip");
    assert!(
        latest_zip.exists(),
        "vault_latest.db.zip snapshot must exist"
    );
    assert!(report.snapshot_bytes > 0);
    assert!(report.snapshot_uncompressed_bytes >= report.snapshot_bytes);

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

    let latest_zip = sync_target.join("snapshots").join("vault_latest.db.zip");
    assert!(
        latest_zip.exists(),
        "vault_latest.db.zip snapshot must exist"
    );
    assert!(
        report1.snapshot_uncompressed_bytes < 250_000,
        "Slim CAS snapshot must dehydrate the 600 KB video BLOB; got {} uncompressed bytes",
        report1.snapshot_uncompressed_bytes
    );

    // Extract vault_latest.db.zip and inspect SQLite directly: synced video payload must be empty x'', local payload retained
    {
        let extracted_db = dir.path().join("extracted_slim.db");
        extract_memories_db_from_zip(&latest_zip, &extracted_db);
        let conn = rusqlite::Connection::open(&extracted_db).unwrap();
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
        report.snapshot_uncompressed_bytes >= payload.len() as u64,
        "--backup-only snapshot must retain full attachment BLOBs before compression (got {} bytes)",
        report.snapshot_uncompressed_bytes
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

#[test]
fn test_bidirectional_merge_preserves_concurrent_device_writes_lww_and_links() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("SharedCloudDrive");
    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());

    let mac_db = dir.path().join("mac.db");
    let linux_db = dir.path().join("linux.db");
    let mac = LightMem::open_at(&mac_db, cfg.clone()).unwrap();
    let linux = LightMem::open_at(&linux_db, cfg).unwrap();

    // 1. Mac writes M1 + M2 with explicit link M1 -> M2 and pushes
    let m1 = mac
        .remember(
            "Core API gateway binds port 8080",
            Some(MemoryType::Fact),
            Some("API Gateway".to_string()),
            vec!["infra".to_string()],
            Some(0.95),
        )
        .unwrap();
    let m2 = mac
        .remember(
            "Primary PostgreSQL cluster runs on port 5432",
            Some(MemoryType::Fact),
            Some("PostgreSQL DB".to_string()),
            vec!["db".to_string()],
            Some(0.95),
        )
        .unwrap();
    mac.link(&m1.id, &m2.id, Some("depends_on"), Some(1.5))
        .unwrap();
    mac.sync_push(None, false).unwrap();

    // 2. Linux non-destructively pulls and gets M1, M2, and link
    let p1 = linux.sync_pull(None, false).unwrap();
    assert!(!p1.restored_db);
    assert!(!p1.skipped_up_to_date);
    assert_eq!(p1.merged_memories, 2);
    assert!(linux.get(&m1.id).unwrap().is_some());
    let linux_links = linux.get_links(&m1.id).unwrap();
    assert_eq!(linux_links.len(), 1);
    assert_eq!(linux_links[0].target_id, m2.id);

    // 3. Divergent writes while offline: Mac adds M_mac, Linux adds M_linux and updates M1 with a newer timestamp
    let m_mac = mac
        .remember(
            "macOS daemon uses 64KB chunked streaming",
            Some(MemoryType::Decision),
            Some("Mac Stream".to_string()),
            vec!["mac".to_string()],
            Some(0.9),
        )
        .unwrap();

    let mut m1_updated = linux.get(&m1.id).unwrap().unwrap();
    m1_updated.content = "Core API gateway binds port 8443 with TLS".to_string();
    m1_updated.updated_at += chrono::Duration::seconds(30);
    linux
        .storage()
        .insert_indexed_batch(
            &[m1_updated.clone()],
            &lightmem::HashEmbeddingProvider,
            linux.requested_embedding_identity(),
            false,
        )
        .unwrap();

    let m_linux = linux
        .remember(
            "Linux systemd service runs on ubuntu-box referencing [[API Gateway]]",
            Some(MemoryType::Event),
            Some("Linux Node".to_string()),
            vec!["linux".to_string()],
            Some(0.9),
        )
        .unwrap();

    // Linux pushes -> pull-first sees nothing new (or Linux is pushed as new snapshot) -> push succeeds cleanly
    let linux_push = linux.sync_push(None, false).unwrap();
    assert!(linux_push.sequence >= 2);

    // Mac pulls -> merges M_linux and newer M1 WITHOUT deleting M_mac or breaking existing M1->M2 link!
    let mac_pull = mac.sync_pull(None, false).unwrap();
    assert!(!mac_pull.skipped_up_to_date);
    assert_eq!(mac_pull.merged_memories, 1);
    assert_eq!(mac_pull.updated_memories, 1);
    assert_eq!(
        mac.get(&m1.id).unwrap().unwrap().content,
        "Core API gateway binds port 8443 with TLS"
    );
    assert!(
        mac.get(&m_mac.id).unwrap().is_some(),
        "Unsynced concurrent local memory on Mac must survive pull merge"
    );
    assert!(mac.get(&m_linux.id).unwrap().is_some());
    assert!(
        mac.get_links(&m1.id)
            .unwrap()
            .iter()
            .any(|l| l.target_id == m2.id && l.relation == "depends_on"),
        "Graph links on updated memory M1 must survive LWW update"
    );

    // Mac pushes combined vault -> Linux pulls -> converges to identical 4 active memories
    mac.sync_push(None, false).unwrap();
    let linux_pull = linux.sync_pull(None, false).unwrap();
    assert!(!linux_pull.skipped_up_to_date);
    assert_eq!(linux_pull.merged_memories, 1);

    let mac_all = mac.list(None, Some(MemoryStatus::Active), None, 0).unwrap();
    let linux_all = linux
        .list(None, Some(MemoryStatus::Active), None, 0)
        .unwrap();
    assert_eq!(mac_all.len(), 4);
    assert_eq!(linux_all.len(), 4);
    assert!(linux.get(&m_mac.id).unwrap().is_some());
}

#[test]
fn test_sync_pull_status_first_skips_when_in_sync_and_merges_when_remote_ahead() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("StatusFirstTarget");
    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());

    let dev_a = LightMem::open_at(&dir.path().join("dev_a.db"), cfg.clone()).unwrap();
    let dev_b = LightMem::open_at(&dir.path().join("dev_b.db"), cfg).unwrap();

    dev_a
        .remember(
            "Initial shared fact from Device A",
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
        )
        .unwrap();
    dev_a.sync_push(None, false).unwrap();

    // Device A is already at remote sha256 -> status reports in_sync and pull skips download
    let st_a = dev_a.sync_status(None).unwrap();
    assert!(st_a.in_sync);
    assert!(!st_a.remote_ahead);
    let pull_a_noop = dev_a.sync_pull(None, false).unwrap();
    assert!(pull_a_noop.skipped_up_to_date);

    // Device B pulls -> gets Device A's memory, then adds a memory and pushes
    let pull_b = dev_b.sync_pull(None, false).unwrap();
    assert!(!pull_b.skipped_up_to_date);
    assert_eq!(pull_b.merged_memories, 1);

    let rec_b = dev_b
        .remember(
            "Second shared fact from Device B",
            Some(MemoryType::Fact),
            None,
            vec![],
            Some(0.9),
        )
        .unwrap();
    dev_b.sync_push(None, false).unwrap();

    // Now Device A status must detect remote_ahead and sync_pull merges cleanly
    let st_a2 = dev_a.sync_status(None).unwrap();
    assert!(st_a2.remote_ahead);
    assert!(!st_a2.in_sync);

    let pull_a2 = dev_a.sync_pull(None, false).unwrap();
    assert!(!pull_a2.skipped_up_to_date);
    assert_eq!(pull_a2.merged_memories, 1);
    assert!(dev_a.get(&rec_b.id).unwrap().is_some());

    // Subsequent pull is a fast skip again
    let pull_a3 = dev_a.sync_pull(None, false).unwrap();
    assert!(pull_a3.skipped_up_to_date);
}

#[test]
fn test_sync_hard_delete_tombstone_propagation_and_resurrection_across_devices() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("TombstoneTarget");
    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());

    let mac = LightMem::open_at(&dir.path().join("mac.db"), cfg.clone()).unwrap();
    let linux = LightMem::open_at(&dir.path().join("linux.db"), cfg).unwrap();

    let m_delete = mac
        .remember(
            "Temporary secret token to hard-delete later",
            Some(MemoryType::Fact),
            Some("Temp Secret".to_string()),
            vec![],
            Some(0.9),
        )
        .unwrap();
    let m_keep = mac
        .remember(
            "Permanent config record",
            Some(MemoryType::Fact),
            Some("Keep Record".to_string()),
            vec![],
            Some(0.95),
        )
        .unwrap();
    mac.sync_push(None, false).unwrap();

    linux.sync_pull(None, false).unwrap();
    assert!(linux.get(&m_delete.id).unwrap().is_some());

    // Linux hard-deletes m_delete and pushes tombstone
    assert!(linux.forget(&m_delete.id, true).unwrap());
    assert!(linux.storage().tombstone_count().unwrap() >= 1);
    linux.sync_push(None, false).unwrap();

    // Mac pulls: m_delete must disappear from Mac while m_keep stays intact
    let pull_del = mac.sync_pull(None, false).unwrap();
    assert_eq!(pull_del.deleted_memories, 1);
    assert!(mac.get(&m_delete.id).unwrap().is_none());
    assert!(mac.get(&m_keep.id).unwrap().is_some());

    // Resurrection: later write reusing the deleted ID with newer timestamp overrides older tombstone
    let mut resurrected = MemoryRecord::new(
        MemoryType::Fact,
        "Resurrected Secret".to_string(),
        "Fresh rotated secret value".to_string(),
        vec!["rotated".to_string()],
        0.95,
        None,
    );
    resurrected.id = m_delete.id.clone();
    resurrected.created_at = chrono::Utc::now() + chrono::Duration::seconds(30);
    resurrected.updated_at = resurrected.created_at;
    mac.storage()
        .insert_indexed_batch(
            &[resurrected.clone()],
            &lightmem::HashEmbeddingProvider,
            mac.requested_embedding_identity(),
            false,
        )
        .unwrap();
    mac.sync_push(None, false).unwrap();

    let pull_res = linux.sync_pull(None, false).unwrap();
    assert_eq!(pull_res.merged_memories, 1);
    let linux_fetched = linux.get(&m_delete.id).unwrap().unwrap();
    assert_eq!(linux_fetched.content, "Fresh rotated secret value");
}

#[test]
fn test_sync_local_memory_never_leaks_and_survives_bidirectional_pull() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("LocalIsolationTarget");
    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());

    let dev_a = LightMem::open_at(&dir.path().join("a.db"), cfg.clone()).unwrap();
    let dev_b = LightMem::open_at(&dir.path().join("b.db"), cfg).unwrap();

    let secret_file = dir.path().join("private_key.txt");
    fs::write(&secret_file, b"SSH PRIVATE KEY MATERIAL").unwrap();

    let (local_rec, _, _) = dev_a
        .remember_with_attachments_and_entities(
            Some("Device A private ssh key"),
            &[secret_file],
            false,
            false,
            None,
            Some(MemoryType::Password),
            Some("Local SSH Key".to_string()),
            vec![],
            Some(1.0),
            None,
            false,
            None,
            true, // --local air-gap
        )
        .unwrap();

    let shared_b = dev_b
        .remember(
            "Shared production hostname db.internal",
            Some(MemoryType::Fact),
            Some("Prod DB".to_string()),
            vec![],
            Some(0.95),
        )
        .unwrap();
    dev_b.sync_push(None, false).unwrap();

    // Device A pulls from Device B: local memory must stay intact while shared_b merges in
    let pull_a = dev_a.sync_pull(None, false).unwrap();
    assert_eq!(pull_a.merged_memories, 1);
    assert!(dev_a.get(&local_rec.id).unwrap().is_some());
    assert!(dev_a.get(&shared_b.id).unwrap().is_some());

    // Device A pushes back: --local memory + its attachment must NOT leak to target or Device B
    let shared_a = dev_a
        .remember(
            "Shared API rate limit is 1000 rpm",
            Some(MemoryType::Fact),
            Some("Rate Limit".to_string()),
            vec![],
            Some(0.9),
        )
        .unwrap();
    let push_a = dev_a.sync_push(None, false).unwrap();
    assert_eq!(push_a.local_only_memories, 1);
    assert_eq!(push_a.blobs_written, 0);

    dev_b.sync_pull(None, false).unwrap();
    assert!(dev_b.get(&shared_a.id).unwrap().is_some());
    assert!(
        dev_b.get(&local_rec.id).unwrap().is_none(),
        "--local memory from Device A must NEVER propagate to Device B"
    );
}

#[test]
fn test_sync_portable_config_adoption_preserves_secrets() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("PortableCfgTarget");

    let mut cfg_mac = hash_config();
    cfg_mac.sync_target = Some(sync_target.to_string_lossy().to_string());
    cfg_mac.reranker = "ollama:gemma4:31b-cloud".to_string();
    cfg_mac.ollama_url = "https://mac-private.local:11434".to_string();
    cfg_mac.ollama_api_key = Some("super-secret-mac-token".to_string());

    let mac = LightMem::open_at(&dir.path().join("mac.db"), cfg_mac).unwrap();
    mac.remember(
        "Portable config sync check",
        Some(MemoryType::Fact),
        None,
        vec![],
        Some(0.9),
    )
    .unwrap();
    mac.sync_push(None, false).unwrap();

    // Inspect manifest.json directly to ensure NO secrets or private Ollama URLs were uploaded
    let manifest_raw = fs::read_to_string(sync_target.join("manifest.json")).unwrap();
    assert!(
        !manifest_raw.contains("super-secret-mac-token"),
        "manifest.json must never leak ollama_api_key"
    );
    assert!(
        !manifest_raw.contains("mac-private.local"),
        "manifest.json must never leak machine-specific ollama_url"
    );
    let manifest_val: serde_json::Value = serde_json::from_str(&manifest_raw).unwrap();
    assert_eq!(
        manifest_val["portable_config"]["reranker"],
        "ollama:gemma4:31b-cloud"
    );

    // Device B (Linux) pulls and adopts portable config while keeping its own local secrets/URL
    let mut cfg_linux = hash_config();
    cfg_linux.sync_target = Some(sync_target.to_string_lossy().to_string());
    cfg_linux.reranker = "top1".to_string();
    cfg_linux.ollama_url = "http://linux-ollama:11434".to_string();
    cfg_linux.ollama_api_key = Some("linux-secret-token".to_string());

    let mut linux = LightMem::open_at(&dir.path().join("linux.db"), cfg_linux).unwrap();
    let (_pull, adopted) = linux.sync_pull_and_adopt(None, false).unwrap();
    assert!(adopted);
    assert_eq!(linux.config().reranker, "ollama:gemma4:31b-cloud");
    assert_eq!(linux.config().ollama_url, "http://linux-ollama:11434");
    assert_eq!(
        linux.config().ollama_api_key.as_deref(),
        Some("linux-secret-token")
    );
}

#[test]
fn test_sync_offline_online_mode_and_cli_auto_pre_pull_post_push() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("CliCloudTarget");
    let sandbox_cwd = dir.path().join("empty_cwd");
    let mac_home = dir.path().join("mac_home");
    let mac_cfg_dir = mac_home.join(".lightmem");
    let linux_home = dir.path().join("linux_home");
    let linux_cfg_dir = linux_home.join(".lightmem");
    fs::create_dir_all(&sandbox_cwd).unwrap();
    fs::create_dir_all(&mac_cfg_dir).unwrap();
    fs::create_dir_all(&linux_cfg_dir).unwrap();

    let mut base_cfg = hash_config();
    base_cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    base_cfg.sync_auto = Some(true);
    base_cfg.sync_offline = Some(false);
    fs::write(
        mac_cfg_dir.join("config.json"),
        serde_json::to_string_pretty(&base_cfg).unwrap(),
    )
    .unwrap();
    fs::write(
        linux_cfg_dir.join("config.json"),
        serde_json::to_string_pretty(&base_cfg).unwrap(),
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_lmem");
    let run_cli = |home: &Path, cfg_dir: &Path, args: &[&str]| {
        Command::new(bin)
            .current_dir(&sandbox_cwd)
            .env("HOME", home)
            .env("LIGHTMEM_HOME", cfg_dir)
            .env("LIGHTMEM_CONFIG_DIR", cfg_dir)
            .env("LIGHTMEM_QA_DISABLE", "1")
            .env_remove("LIGHTMEM_DB")
            .env_remove("LIGHTMEM_CONNECTION")
            .env_remove("LIGHTMEM_SYNC_TARGET")
            .env_remove("LIGHTMEM_SYNC_AUTO")
            .env_remove("LIGHTMEM_OFFLINE")
            .args(args)
            .output()
            .unwrap()
    };

    // 1. Mac remembers M1 with --json -> stdout must remain pure JSON AND auto-push to sync_target
    let out_mac1 = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["remember", "Decision from Mac Studio", "--json"],
    );
    assert!(
        out_mac1.status.success(),
        "Mac remember failed: {}",
        String::from_utf8_lossy(&out_mac1.stderr)
    );
    let mac1_json: serde_json::Value = serde_json::from_slice(&out_mac1.stdout).unwrap();
    let mac1_id = mac1_json["id"].as_str().unwrap().to_string();
    assert!(
        sync_target
            .join("snapshots")
            .join("vault_latest.db.zip")
            .exists(),
        "Auto post-write push must create vault_latest.db.zip"
    );

    // 2. Linux remembers M2 WITHOUT manually running sync first:
    // pre-write pull must merge Mac's M1 before saving M2, and post-write push uploads both!
    let out_lin1 = run_cli(
        &linux_home,
        &linux_cfg_dir,
        &["remember", "Event from Linux Box", "--json"],
    );
    assert!(
        out_lin1.status.success(),
        "Linux remember failed: {}",
        String::from_utf8_lossy(&out_lin1.stderr)
    );
    let lin_list_out = run_cli(&linux_home, &linux_cfg_dir, &["list", "--json"]);
    assert!(lin_list_out.status.success());
    let lin_list_json: serde_json::Value = serde_json::from_slice(&lin_list_out.stdout).unwrap();
    assert_eq!(lin_list_json["total"], 2);
    let lin_ids: Vec<&str> = lin_list_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(lin_ids.contains(&mac1_id.as_str()));

    // 3. Explicit --db vault must NOT sync to the global target unless LIGHTMEM_SYNC_TARGET is set
    let explicit_db = dir.path().join("explicit_isolated.db");
    let explicit_out = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &[
            "--db",
            explicit_db.to_str().unwrap(),
            "remember",
            "Isolated explicit DB note",
            "--json",
        ],
    );
    assert!(explicit_out.status.success());
    let explicit_json: serde_json::Value = serde_json::from_slice(&explicit_out.stdout).unwrap();
    let explicit_id = explicit_json["id"].as_str().unwrap().to_string();

    // Verify Linux does NOT receive explicit_id on pull
    let lin_pull_out = run_cli(&linux_home, &linux_cfg_dir, &["sync", "--pull", "--json"]);
    assert!(lin_pull_out.status.success());
    let lin_list_after_explicit = run_cli(&linux_home, &linux_cfg_dir, &["list", "--json"]);
    let lin_after_val: serde_json::Value =
        serde_json::from_slice(&lin_list_after_explicit.stdout).unwrap();
    assert_eq!(lin_after_val["total"], 2);
    assert!(!lin_after_val["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"].as_str() == Some(explicit_id.as_str())));

    // 4. Toggle OFFLINE mode on Mac: writes stay local and sync --status is offline-safe
    let off_out = run_cli(&mac_home, &mac_cfg_dir, &["sync", "--offline", "--json"]);
    assert!(off_out.status.success());
    let off_json: serde_json::Value = serde_json::from_slice(&off_out.stdout).unwrap();
    assert_eq!(off_json["sync_offline"], true);

    let off_note = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["remember", "Offline note created on plane", "--json"],
    );
    assert!(off_note.status.success());

    let status_off = run_cli(&mac_home, &mac_cfg_dir, &["sync", "--status", "--json"]);
    assert!(status_off.status.success());
    let status_off_json: serde_json::Value = serde_json::from_slice(&status_off.stdout).unwrap();
    assert_eq!(status_off_json["offline"], true);
    assert_eq!(status_off_json["remote_check_skipped_offline"], true);
    assert_eq!(status_off_json["local_dirty"], true);

    // Linux should NOT have seen Mac's offline note yet
    let lin_pull_off = run_cli(&linux_home, &linux_cfg_dir, &["sync", "--pull", "--json"]);
    assert!(lin_pull_off.status.success());
    let lin_pull_off_json: serde_json::Value =
        serde_json::from_slice(&lin_pull_off.stdout).unwrap();
    assert_eq!(lin_pull_off_json["skipped_up_to_date"], true);

    // 5. Resume ONLINE mode on Mac -> next remember auto-pulls Linux's M2 and pushes Mac's offline note
    let on_out = run_cli(&mac_home, &mac_cfg_dir, &["sync", "--online", "--json"]);
    assert!(on_out.status.success());
    let on_json: serde_json::Value = serde_json::from_slice(&on_out.stdout).unwrap();
    assert_eq!(on_json["sync_offline"], false);

    let back_online = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["remember", "Landed back online", "--json"],
    );
    assert!(back_online.status.success());

    // Linux pulls and sees all 4 global memories!
    let lin_pull_final = run_cli(&linux_home, &linux_cfg_dir, &["sync", "--pull", "--json"]);
    assert!(lin_pull_final.status.success());
    let lin_list_final = run_cli(&linux_home, &linux_cfg_dir, &["list", "--json"]);
    let lin_final_json: serde_json::Value = serde_json::from_slice(&lin_list_final.stdout).unwrap();
    assert_eq!(lin_final_json["total"], 4);

    // 6. Test `lmem config --offline <bool> --json`
    let cfg_off = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["config", "--offline", "true", "--json"],
    );
    assert!(cfg_off.status.success());
    let cfg_off_json: serde_json::Value = serde_json::from_slice(&cfg_off.stdout).unwrap();
    assert_eq!(cfg_off_json["effective_sync_offline"], true);

    let cfg_on = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["config", "--offline", "false", "--json"],
    );
    assert!(cfg_on.status.success());
    let cfg_on_json: serde_json::Value = serde_json::from_slice(&cfg_on.stdout).unwrap();
    assert_eq!(cfg_on_json["effective_sync_offline"], false);

    // 7. Linux hard-forgets mac1_id via CLI -> auto-pushes tombstone -> Mac auto-pulls tombstone on next write
    let lin_forget = run_cli(
        &linux_home,
        &linux_cfg_dir,
        &["forget", &mac1_id, "--hard", "--json"],
    );
    assert!(
        lin_forget.status.success(),
        "Linux forget failed: {}",
        String::from_utf8_lossy(&lin_forget.stderr)
    );

    let mac_after_forget = run_cli(
        &mac_home,
        &mac_cfg_dir,
        &["remember", "Post-delete Mac memory", "--json"],
    );
    assert!(mac_after_forget.status.success());

    let mac_list_after = run_cli(&mac_home, &mac_cfg_dir, &["list", "--json"]);
    let mac_list_val: serde_json::Value = serde_json::from_slice(&mac_list_after.stdout).unwrap();
    assert_eq!(mac_list_val["total"], 4); // 4 - 1 deleted + 1 added = 4
    assert!(!mac_list_val["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"].as_str() == Some(mac1_id.as_str())));
}

#[cfg(feature = "mcp")]
#[test]
fn test_mcp_offline_config_and_local_remember_with_auto_sync() {
    let dir = tempdir().unwrap();
    let sync_target = dir.path().join("McpCloudTarget");
    let sandbox_cwd = dir.path().join("mcp_cwd");
    let home = dir.path().join("mcp_home");
    let cfg_dir = home.join(".lightmem");
    fs::create_dir_all(&sandbox_cwd).unwrap();
    fs::create_dir_all(&cfg_dir).unwrap();

    let mut cfg = hash_config();
    cfg.sync_target = Some(sync_target.to_string_lossy().to_string());
    cfg.sync_auto = Some(true);
    cfg.sync_offline = Some(false);
    fs::write(
        cfg_dir.join("config.json"),
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_lmem"))
        .current_dir(&sandbox_cwd)
        .env("HOME", &home)
        .env("LIGHTMEM_HOME", &cfg_dir)
        .env("LIGHTMEM_CONFIG_DIR", &cfg_dir)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION")
        .env_remove("LIGHTMEM_SYNC_TARGET")
        .env_remove("LIGHTMEM_SYNC_AUTO")
        .env_remove("LIGHTMEM_OFFLINE")
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        for req in [
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "remember", "arguments": { "content": "Shared MCP fact", "category": "fact" } }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "remember", "arguments": { "content": "Air-gapped MCP secret", "category": "password", "local": true } }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "config", "arguments": { "offline": true } }
            }),
        ] {
            writeln!(stdin, "{}", req).unwrap();
        }
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "MCP server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);

    // Local remember returned #local provenance
    let local_text = lines[1]["result"]["content"][0]["text"].as_str().unwrap();
    let local_val: serde_json::Value = serde_json::from_str(local_text).unwrap();
    assert_eq!(local_val["memory"]["provenance"], "local");

    // Config tool toggled sync_offline = true
    let cfg_text = lines[2]["result"]["content"][0]["text"].as_str().unwrap();
    let cfg_val: serde_json::Value = serde_json::from_str(cfg_text).unwrap();
    assert_eq!(cfg_val["updated"], true);
    assert_eq!(cfg_val["sync_offline"], true);

    // Sync snapshot exists and does NOT contain the --local MCP secret!
    let snap_zip = sync_target.join("snapshots").join("vault_latest.db.zip");
    assert!(snap_zip.exists());
    let extracted = dir.path().join("mcp_snap.db");
    extract_memories_db_from_zip(&snap_zip, &extracted);
    let snap_storage = Storage::open(&extracted).unwrap();
    let snap_mems = snap_storage.list_memories(None, None, None, 0).unwrap();
    assert_eq!(snap_mems.len(), 1);
    assert_eq!(snap_mems[0].content, "Shared MCP fact");
}
