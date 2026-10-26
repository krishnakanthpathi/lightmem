use lightmem::{
    compute_sha256_hex, BundleService, LightMem, LightMemConfig, MemoryType,
};
use std::fs::File;
use std::io::Read;
use tempfile::tempdir;
use zip::ZipArchive;

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

fn make_fake_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn make_fake_mp4(data_payload_len: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    // ftyp box
    let ftyp_size = 20u32;
    bytes.extend_from_slice(&ftyp_size.to_be_bytes());
    bytes.extend_from_slice(b"ftyp");
    bytes.extend_from_slice(b"isom");
    bytes.extend_from_slice(&0x00000200u32.to_be_bytes());
    bytes.extend_from_slice(b"isom");

    // moov box
    let moov_size = 8u32;
    bytes.extend_from_slice(&moov_size.to_be_bytes());
    bytes.extend_from_slice(b"moov");

    // mdat box (media data)
    let mdat_size = (8 + data_payload_len) as u32;
    bytes.extend_from_slice(&mdat_size.to_be_bytes());
    bytes.extend_from_slice(b"mdat");
    bytes.extend(std::iter::repeat_n(0x42u8, data_payload_len));
    bytes
}

#[test]
fn test_bundle_export_and_restore_with_video_and_photos() {
    let src_dir = tempdir().unwrap();
    let src_db = src_dir.path().join("source_vault.db");
    let lm_src = LightMem::open_at(&src_db, hash_config()).unwrap();

    // 1. Create real files on disk to attach
    let fake_video_data = make_fake_mp4(1024 * 50); // 50KB video
    let fake_video_sha = compute_sha256_hex(&fake_video_data);
    let video_path = src_dir.path().join("test_demo.mp4");
    std::fs::write(&video_path, &fake_video_data).unwrap();

    let fake_photo_data = make_fake_png(800, 600);
    let fake_photo_sha = compute_sha256_hex(&fake_photo_data);
    let photo_path = src_dir.path().join("diagram.png");
    std::fs::write(&photo_path, &fake_photo_data).unwrap();

    let files_to_attach = vec![photo_path, video_path];
    let (mem1, _) = lm_src
        .remember_with_attachments(
            Some("Release Architecture Walkthrough with Attached Video and Diagram"),
            &files_to_attach,
            false,
            None,
            Some(MemoryType::Artifact),
            Some("Release Walkthrough".to_string()),
            vec!["architecture".to_string(), "release".to_string()],
            Some(0.95),
            None,
            false,
            None,
        )
        .unwrap();

    let mem2 = lm_src
        .remember(
            "Core Design Decision: Use SQLite WAL and single file zip bundles",
            Some(MemoryType::Decision),
            Some("Bundle Decision".to_string()),
            vec!["design".to_string()],
            Some(0.98),
        )
        .unwrap();

    // Create a link between them
    lm_src
        .link(&mem1.id, &mem2.id, Some("explains"), Some(1.0))
        .unwrap();

    // 2. Export to a single .zip transfer bundle
    let bundle_path = src_dir.path().join("full_transfer.zip");
    let exported = lm_src.export_bundle(Some(&bundle_path)).unwrap();
    assert_eq!(exported, bundle_path);
    assert!(bundle_path.exists());

    // 3. Inspect the bundle without unpacking
    let manifest = BundleService::inspect_bundle(&bundle_path).unwrap();
    assert_eq!(manifest.format, "lightmem-bundle-v1");
    assert_eq!(manifest.stats.total_memories, 2);
    assert_eq!(manifest.stats.total_links, 1);
    assert_eq!(manifest.stats.total_attachments, 2);
    assert_eq!(manifest.stats.media_breakdown.get("video").copied(), Some(1));
    assert_eq!(manifest.stats.media_breakdown.get("image").copied(), Some(1));
    assert_eq!(manifest.attachments.len(), 2);

    // 4. Verify ZIP archive structure directly
    {
        let file = File::open(&bundle_path).unwrap();
        let mut zip = ZipArchive::new(file).unwrap();

        // Must contain manifest.json
        assert!(zip.by_name("manifest.json").is_ok());

        // Must contain memories.db
        assert!(zip.by_name("memories.db").is_ok());

        // Must contain physical video and photo files inside attachments/
        {
            let mut video_file = zip.by_name("attachments/test_demo.mp4").unwrap();
            let mut video_bytes = Vec::new();
            video_file.read_to_end(&mut video_bytes).unwrap();
            assert_eq!(video_bytes, fake_video_data);
            assert_eq!(compute_sha256_hex(&video_bytes), fake_video_sha);
        }

        {
            let mut photo_file = zip.by_name("attachments/diagram.png").unwrap();
            let mut photo_bytes = Vec::new();
            photo_file.read_to_end(&mut photo_bytes).unwrap();
            assert_eq!(photo_bytes, fake_photo_data);
            assert_eq!(compute_sha256_hex(&photo_bytes), fake_photo_sha);
        }
    }

    // 5. Restore into a completely fresh destination vault
    let dst_dir = tempdir().unwrap();
    let dst_db = dst_dir.path().join("destination_vault.db");
    let lm_dst = LightMem::open_at(&dst_db, hash_config()).unwrap();

    let res = lm_dst
        .import_bundle(&bundle_path, false, false)
        .unwrap();
    assert_eq!(res.mode, "restore");
    assert_eq!(res.memories_imported, 2);
    assert_eq!(res.attachments_extracted, 2);

    // Verify all memories restored
    let memories = lm_dst.list(None, None, None, 10).unwrap();
    assert_eq!(memories.len(), 2);

    // Verify links restored
    let graph = lm_dst.graph(None, None).unwrap();
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0].relation, "explains");

    // Verify attachments were materialized onto destination disk
    let attachments_dir = dst_dir.path().join("attachments");
    assert!(attachments_dir.exists());

    let extracted_video = attachments_dir.join("test_demo.mp4");
    assert!(extracted_video.exists());
    assert_eq!(std::fs::read(&extracted_video).unwrap(), fake_video_data);

    let extracted_photo = attachments_dir.join("diagram.png");
    assert!(extracted_photo.exists());
    assert_eq!(std::fs::read(&extracted_photo).unwrap(), fake_photo_data);

    // Verify DB attachments query returns verified payloads
    let mem1_atts = lm_dst.get_attachments_data(&mem1.id).unwrap();
    assert_eq!(mem1_atts.len(), 2);
}

#[test]
fn test_bundle_merge_into_existing_vault() {
    let dir1 = tempdir().unwrap();
    let db1 = dir1.path().join("vault1.db");
    let lm1 = LightMem::open_at(&db1, hash_config()).unwrap();

    let fake_video = make_fake_mp4(2048);
    let video_path = dir1.path().join("clip1.mp4");
    std::fs::write(&video_path, &fake_video).unwrap();

    let files = vec![video_path];
    lm1.remember_with_attachments(
        Some("Vault 1 Video Memory"),
        &files,
        false,
        None,
        None,
        Some("Clip 1".to_string()),
        vec!["video".to_string()],
        Some(0.9),
        None,
        false,
        None,
    )
    .unwrap();

    let bundle1 = dir1.path().join("v1.zip");
    lm1.export_bundle(Some(&bundle1)).unwrap();

    // Vault 2 has existing memories
    let dir2 = tempdir().unwrap();
    let db2 = dir2.path().join("vault2.db");
    let lm2 = LightMem::open_at(&db2, hash_config()).unwrap();
    lm2.remember("Vault 2 Initial Fact", None, None, vec![], None)
        .unwrap();

    // Attempting import without --replace or --merge fails safely
    let err = lm2.import_bundle(&bundle1, false, false);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("already contains"));

    // Merging into Vault 2 succeeds
    let res = lm2.import_bundle(&bundle1, false, true).unwrap();
    assert_eq!(res.mode, "merged");
    assert_eq!(res.memories_imported, 1);

    // Vault 2 now has 2 memories
    let count = lm2.count(None, None, None).unwrap();
    assert_eq!(count, 2);

    // Vault 2 attachments folder has clip1.mp4
    let att_dir = dir2.path().join("attachments");
    assert!(att_dir.join("clip1.mp4").exists());
    assert_eq!(std::fs::read(att_dir.join("clip1.mp4")).unwrap(), fake_video);
}

#[test]
fn test_cli_bundle_commands_end_to_end() {
    use std::process::Command;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_vault.db");
    let video_path = dir.path().join("rec.mp4");
    std::fs::write(&video_path, make_fake_mp4(1024)).unwrap();

    let exe = env!("CARGO_BIN_EXE_lmem");

    // 1. Remember a fact with video attachment via CLI
    let status = Command::new(exe)
        .arg("--db")
        .arg(&db_path)
        .arg("remember")
        .arg("CLI Video Project Memory")
        .arg("-f")
        .arg(&video_path)
        .status()
        .unwrap();
    assert!(status.success());

    // 2. Export via `lmem bundle export -o bundle.zip`
    let bundle_file = dir.path().join("cli_exported.zip");
    let output = Command::new(exe)
        .arg("--db")
        .arg(&db_path)
        .arg("bundle")
        .arg("export")
        .arg("-o")
        .arg(&bundle_file)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(bundle_file.exists());

    // 3. Info via `lmem bundle info bundle.zip --json`
    let info_output = Command::new(exe)
        .arg("bundle")
        .arg("info")
        .arg(&bundle_file)
        .arg("--json")
        .output()
        .unwrap();
    assert!(info_output.status.success());
    let info_json: serde_json::Value =
        serde_json::from_slice(&info_output.stdout).expect("Valid manifest JSON");
    assert_eq!(info_json["stats"]["total_memories"], 1);
    assert_eq!(info_json["stats"]["total_attachments"], 1);
    assert_eq!(info_json["stats"]["media_breakdown"]["video"], 1);

    // 4. Restore into another DB via `lmem import bundle.zip`
    let target_db = dir.path().join("restored.db");
    let import_output = Command::new(exe)
        .arg("--db")
        .arg(&target_db)
        .arg("import")
        .arg(&bundle_file)
        .arg("--json")
        .output()
        .unwrap();
    assert!(import_output.status.success());

    let restored_att_dir = dir.path().join("attachments");
    assert!(restored_att_dir.join("rec.mp4").exists());
}
