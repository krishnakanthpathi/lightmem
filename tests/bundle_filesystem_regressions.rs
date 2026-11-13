use lightmem::{BundleService, Exporter, Storage};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn bundle(dir: &Path, omit_last: bool, corrupt_last: bool) -> PathBuf {
    let source = Storage::open(dir.join("source.db")).unwrap();
    let record = lightmem::MemoryRecord::new(
        lightmem::MemoryType::Fact,
        "source".into(),
        "incoming fact".into(),
        vec![],
        0.9,
        None,
    );
    source.insert_memory(&record, None).unwrap();
    let original = BundleService::export_bundle(&source, Some(&dir.join("original.zip"))).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(original).unwrap()).unwrap();
    let mut manifest: lightmem::BundleManifest =
        BundleService::inspect_bundle(&dir.join("original.zip")).unwrap();
    for filename in ["first.txt", "last.txt"] {
        manifest.attachments.push(lightmem::BundleAttachmentEntry {
            id: filename.into(),
            memory_id: record.id.clone(),
            filename: filename.into(),
            entry_path: format!("attachments/{filename}"),
            mime_type: "text/plain".into(),
            media_kind: "document".into(),
            size_bytes: 8,
            sha256: lightmem::compute_sha256_hex(b"incoming"),
            dimensions: None,
            duration_secs: None,
        });
    }
    manifest.stats.total_attachments = 2;
    manifest.stats.total_attachment_bytes = 16;
    let path = dir.join("bundle.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    let mut db = Vec::new();
    archive
        .by_name("memories.db")
        .unwrap()
        .read_to_end(&mut db)
        .unwrap();
    zip.start_file("memories.db", options).unwrap();
    zip.write_all(&db).unwrap();
    for filename in ["first.txt", "last.txt"] {
        if omit_last && filename == "last.txt" {
            continue;
        }
        zip.start_file(format!("attachments/{filename}"), options)
            .unwrap();
        zip.write_all(if corrupt_last && filename == "last.txt" {
            b"tampered"
        } else {
            b"incoming"
        })
        .unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn refused_occupied_import_preserves_attachments() {
    let source = tempfile::tempdir().unwrap();
    let bundle = bundle(source.path(), false, false);
    let target = tempfile::tempdir().unwrap();
    let storage = Storage::open(target.path().join("vault.db")).unwrap();
    storage
        .insert_memory(
            &lightmem::MemoryRecord::new(
                lightmem::MemoryType::Fact,
                "original".into(),
                "original fact".into(),
                vec![],
                0.9,
                None,
            ),
            None,
        )
        .unwrap();
    std::fs::create_dir(target.path().join("attachments")).unwrap();
    let file = target.path().join("attachments/first.txt");
    std::fs::write(&file, b"original").unwrap();
    assert!(BundleService::import_bundle(&storage, &bundle, false, false).is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"original");
    assert!(!target.path().join("attachments/last.txt").exists());
}

#[test]
fn incomplete_or_corrupt_bundle_changes_neither_files_nor_database() {
    for (missing, corrupt) in [(true, false), (false, true)] {
        let source = tempfile::tempdir().unwrap();
        let bundle = bundle(source.path(), missing, corrupt);
        let target = tempfile::tempdir().unwrap();
        let storage = Storage::open(target.path().join("vault.db")).unwrap();
        std::fs::create_dir(target.path().join("attachments")).unwrap();
        let original = target.path().join("attachments/first.txt");
        std::fs::write(&original, b"original").unwrap();
        let result = BundleService::import_bundle(&storage, &bundle, false, false);
        assert!(result.is_err(), "incomplete/corrupt bundle accepted");
        assert_eq!(std::fs::read(original).unwrap(), b"original");
        assert_eq!(storage.count_memories(None, None, None).unwrap(), 0);
        assert!(!target.path().join("attachments/last.txt").exists());
    }
}

#[test]
fn failed_database_merge_rolls_back_attachment_publication() {
    let source = tempfile::tempdir().unwrap();
    let bundle = bundle(source.path(), false, false);
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("vault.db");
    let storage = Storage::open(&path).unwrap();
    rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_insert BEFORE INSERT ON memories BEGIN SELECT RAISE(ABORT, 'injected merge failure'); END;").unwrap();
    std::fs::create_dir(target.path().join("attachments")).unwrap();
    let original = target.path().join("attachments/first.txt");
    std::fs::write(&original, b"original").unwrap();
    assert!(BundleService::import_bundle(&storage, &bundle, false, true).is_err());
    assert_eq!(std::fs::read(original).unwrap(), b"original");
    assert!(!target.path().join("attachments/last.txt").exists());
    assert_eq!(storage.count_memories(None, None, None).unwrap(), 0);
}

#[cfg(unix)]
#[test]
fn import_never_follows_leaf_or_attachment_directory_symlinks() {
    use std::os::unix::fs::symlink;
    for parent_link in [false, true] {
        let source = tempfile::tempdir().unwrap();
        let bundle = bundle(source.path(), false, false);
        let target = tempfile::tempdir().unwrap();
        let storage = Storage::open(target.path().join("vault.db")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("first.txt");
        std::fs::write(&victim, b"outside original").unwrap();
        let root = target.path().join("attachments");
        if parent_link {
            symlink(outside.path(), &root).unwrap();
        } else {
            std::fs::create_dir(&root).unwrap();
            symlink(&victim, root.join("first.txt")).unwrap();
        }
        let _result = BundleService::import_bundle(&storage, &bundle, false, false);
        assert_eq!(std::fs::read(&victim).unwrap(), b"outside original");
        assert!(!outside.path().join("last.txt").exists());
    }
}

#[cfg(unix)]
#[test]
fn existing_exports_are_private_and_do_not_follow_leaf_symlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("vault.db")).unwrap();
    for kind in ["json", "okf", "bundle"] {
        for link in [false, true] {
            let victim = dir.path().join(format!("victim-{kind}-{link}"));
            std::fs::write(&victim, b"original").unwrap();
            let output = dir.path().join(format!("export-{kind}-{link}"));
            if link {
                symlink(&victim, &output).unwrap();
            } else {
                std::fs::write(&output, b"old export").unwrap();
                std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            match kind {
                "json" => Exporter::export_json(&storage, &output).unwrap(),
                "okf" => Exporter::export_okf(&storage, Some(&output)).unwrap(),
                _ => BundleService::export_bundle(&storage, Some(&output)).unwrap(),
            };
            assert_eq!(std::fs::read(&victim).unwrap(), b"original");
            assert!(!std::fs::symlink_metadata(&output)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600,
                "{kind}"
            );
        }
    }
}
