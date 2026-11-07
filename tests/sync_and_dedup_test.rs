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
