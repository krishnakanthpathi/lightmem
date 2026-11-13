use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::models::compute_sha256_hex;
use crate::storage::Storage;

const MAX_MANIFEST_SIZE: u64 = 32 * 1024 * 1024; // 32 MB
const MAX_ATTACHMENT_SIZE: u64 = 1024 * 1024 * 1024; // 1 GB
const MAX_PREALLOCATION_BYTES: usize = 32 * 1024 * 1024; // 32 MB
const MAX_DB_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4 GB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStats {
    pub total_memories: usize,
    pub total_links: usize,
    pub total_attachments: usize,
    pub total_attachment_bytes: u64,
    pub media_breakdown: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleAttachmentEntry {
    pub id: String,
    pub memory_id: String,
    pub filename: String,
    pub entry_path: String,
    pub mime_type: String,
    pub media_kind: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub dimensions: Option<String>,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format: String,
    pub lightmem_version: String,
    pub created_at: DateTime<Utc>,
    pub embedding_identity: Option<String>,
    pub db_sha256: String,
    pub db_size_bytes: u64,
    pub stats: BundleStats,
    pub attachments: Vec<BundleAttachmentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleImportResult {
    pub bundle_file: PathBuf,
    pub memories_imported: usize,
    pub attachments_extracted: usize,
    pub total_attachment_bytes: u64,
    pub manifest: BundleManifest,
    pub mode: String, // "restore" or "merged"
}

// Publication uses whole-directory renames: existing files (including symlinks)
// are moved, never opened for writing. On ordinary errors the original tree is
// restored. This is not a crash-atomic transaction with SQLite.
struct AttachmentSwap {
    staging: Option<tempfile::TempDir>,
    destination: PathBuf,
    had_previous: bool,
    published: bool,
    committed: bool,
}

impl AttachmentSwap {
    fn publish(staging: tempfile::TempDir, destination: &Path) -> Result<Self> {
        let mut swap = Self {
            staging: Some(staging),
            destination: destination.into(),
            had_previous: false,
            published: false,
            committed: false,
        };
        let stage = swap.staging.as_ref().unwrap().path().to_path_buf();
        let previous = stage.join("previous");
        match std::fs::rename(destination, &previous) {
            Ok(()) => swap.had_previous = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let next = stage.join("next");
        std::fs::create_dir(&next)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o700))?;
        }
        if swap.had_previous {
            copy_private_tree(&previous, &next)?;
        }
        for entry in std::fs::read_dir(stage.join("incoming"))? {
            let entry = entry?;
            std::fs::rename(entry.path(), next.join(entry.file_name()))?;
        }
        std::fs::rename(&next, destination)?;
        swap.published = true;
        Ok(swap)
    }

    fn rollback(&mut self) -> Result<()> {
        if self.published {
            std::fs::rename(
                &self.destination,
                self.staging.as_ref().unwrap().path().join("failed"),
            )?;
            self.published = false;
        }
        if self.had_previous {
            std::fs::rename(
                self.staging.as_ref().unwrap().path().join("previous"),
                &self.destination,
            )?;
            self.had_previous = false;
        }
        Ok(())
    }
}

impl Drop for AttachmentSwap {
    fn drop(&mut self) {
        if !self.committed && self.rollback().is_err() {
            // Never delete the only surviving original tree on rollback failure.
            if let Some(staging) = self.staging.take() {
                eprintln!(
                    "Attachment rollback failed; recovery data retained at {:?}",
                    staging.keep()
                );
            }
        }
    }
}

fn copy_private_tree(source: &Path, destination: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Attachment root must be a real directory"
    );
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Symlinks are not allowed in the attachment tree"
        );
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            std::fs::create_dir(&target)?;
            copy_private_tree(&entry.path(), &target)?;
        } else {
            anyhow::ensure!(metadata.is_file(), "Non-regular attachment file");
            let mut input = File::open(entry.path())?;
            let mut output = tempfile::NamedTempFile::new_in(destination)?;
            std::io::copy(&mut input, &mut output)?;
            output.persist(target)?;
        }
    }
    Ok(())
}

pub struct BundleService;

impl BundleService {
    /// Export full vault (SQLite DB, all video/photo attachments, graph links, vectors, and manifest) into a single compressed .zip bundle
    pub fn export_bundle(storage: &Storage, output_path: Option<&Path>) -> Result<PathBuf> {
        let target_path = if let Some(p) = output_path {
            p.to_path_buf()
        } else {
            let export_dir = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".lightmem")
                .join("exports");
            std::fs::create_dir_all(&export_dir)?;
            let filename = format!("lightmem_vault_{}.zip", Utc::now().format("%Y%m%d_%H%M%S"));
            export_dir.join(filename)
        };

        if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }

        // 1. Snapshot database cleanly
        let temp_dir = tempfile::tempdir()?;
        let temp_db_path = temp_dir.path().join("memories.db");
        storage.backup_to_file(&temp_db_path)?;

        let db_bytes = std::fs::read(&temp_db_path)?;
        let db_sha256 = compute_sha256_hex(&db_bytes);
        let db_size_bytes = db_bytes.len() as u64;

        // 2. Query statistics & metadata
        let total_memories = storage.count_memories(None, None, None)?;
        let total_links = storage.all_links().unwrap_or_default().len();
        let embedding_identity = storage.embedding_identity()?;

        // 3. Collect all video, image, document attachments
        let attachments = storage.all_attachments_data()?;
        let mut bundle_entries = Vec::with_capacity(attachments.len());
        let mut media_breakdown: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_attachment_bytes = 0u64;
        let mut used_entry_names = HashSet::new();

        for (meta, data) in &attachments {
            total_attachment_bytes += data.len() as u64;
            *media_breakdown.entry(meta.media_kind.clone()).or_default() += 1;

            let safe_name = Path::new(&meta.filename)
                .file_name()
                .and_then(|s| s.to_str())
                .filter(|s| {
                    !s.is_empty() && !s.contains("..") && !s.contains('/') && !s.contains('\\')
                })
                .unwrap_or("attachment.bin");

            let entry_name = if used_entry_names.contains(safe_name) {
                let stem = Path::new(safe_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("attachment");
                let ext = Path::new(safe_name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let short_sha = &meta.sha256[..meta.sha256.len().min(8)];
                if ext.is_empty() {
                    format!("{}_{}", stem, short_sha)
                } else {
                    format!("{}_{}.{}", stem, short_sha, ext)
                }
            } else {
                safe_name.to_string()
            };
            used_entry_names.insert(entry_name.clone());

            let entry_path = format!("attachments/{}", entry_name);
            bundle_entries.push(BundleAttachmentEntry {
                id: meta.id.clone(),
                memory_id: meta.memory_id.clone(),
                filename: meta.filename.clone(),
                entry_path,
                mime_type: meta.mime_type.clone(),
                media_kind: meta.media_kind.clone(),
                size_bytes: meta.size_bytes,
                sha256: meta.sha256.clone(),
                dimensions: meta.dimensions.clone(),
                duration_secs: meta.duration_secs.map(|d| d as f64),
            });
        }

        let manifest = BundleManifest {
            format: "lightmem-bundle-v1".to_string(),
            lightmem_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: Utc::now(),
            embedding_identity,
            db_sha256,
            db_size_bytes,
            stats: BundleStats {
                total_memories,
                total_links,
                total_attachments: attachments.len(),
                total_attachment_bytes,
                media_breakdown,
            },
            attachments: bundle_entries,
        };

        // 4. Create the ZIP archive
        let parent = target_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut staged_export = tempfile::NamedTempFile::new_in(parent)?;
        let mut zip = ZipWriter::new(staged_export.as_file_mut());
        let options_deflate =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        // Write manifest.json
        zip.start_file("manifest.json", options_deflate)?;
        let manifest_bytes = serde_json::to_string_pretty(&manifest)?;
        zip.write_all(manifest_bytes.as_bytes())?;

        // Write memories.db
        zip.start_file("memories.db", options_deflate)?;
        zip.write_all(&db_bytes)?;

        // Write each video, photo, document attachment
        for (entry, (_, data)) in manifest.attachments.iter().zip(attachments.iter()) {
            zip.start_file(&entry.entry_path, options_deflate)?;
            zip.write_all(data)?;
        }

        zip.finish()?;
        staged_export.as_file().sync_all()?;
        staged_export
            .persist(&target_path)
            .with_context(|| format!("Failed to publish bundle at {:?}", target_path))?;
        Ok(target_path)
    }

    /// Read and inspect bundle manifest without extracting files
    pub fn inspect_bundle(bundle_path: &Path) -> Result<BundleManifest> {
        let file = File::open(bundle_path)
            .with_context(|| format!("Failed to open bundle file at {:?}", bundle_path))?;
        let mut archive = ZipArchive::new(file)
            .with_context(|| format!("Invalid ZIP bundle format in {:?}", bundle_path))?;

        let mut manifest_file = archive
            .by_name("manifest.json")
            .with_context(|| "Bundle archive is missing manifest.json")?;
        let mut content = String::new();
        manifest_file
            .by_ref()
            .take(MAX_MANIFEST_SIZE)
            .read_to_string(&mut content)?;

        let manifest: BundleManifest = serde_json::from_str(&content)
            .with_context(|| "Failed to parse manifest.json from bundle")?;
        anyhow::ensure!(
            manifest.format == "lightmem-bundle-v1",
            "Unsupported bundle format: '{}'. Expected 'lightmem-bundle-v1'",
            manifest.format
        );
        Ok(manifest)
    }

    /// Import a bundle: unpacks DB, restores/merges memories and vectors, and materializes all video/photo files
    pub fn import_bundle(
        storage: &Storage,
        bundle_path: &Path,
        replace: bool,
        merge: bool,
    ) -> Result<BundleImportResult> {
        let existing_count = storage.count_memories(None, None, None)?;
        anyhow::ensure!(merge || replace || existing_count == 0,
            "Target database already contains {} memories. Pass --replace to completely overwrite the vault with this transfer bundle, or --merge to combine them.", existing_count);
        let manifest = Self::inspect_bundle(bundle_path)?;

        let file = File::open(bundle_path)?;
        let mut archive = ZipArchive::new(file)?;

        // 1. Extract and verify memories.db
        let parent = storage
            .path()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let attachments_dir = parent.join("attachments");
        let temp_dir = tempfile::Builder::new()
            .prefix(".lightmem-import-")
            .tempdir_in(&parent)?;
        let temp_db_path = temp_dir.path().join("memories.db");
        {
            let mut db_file = archive
                .by_name("memories.db")
                .with_context(|| "Bundle is missing memories.db")?;
            anyhow::ensure!(
                manifest.db_size_bytes <= MAX_DB_SIZE,
                "Database declared size ({} bytes) exceeds maximum allowed limit ({} bytes)",
                manifest.db_size_bytes,
                MAX_DB_SIZE
            );
            let mut out = File::create(&temp_db_path)?;
            let mut limited = db_file.by_ref().take(MAX_DB_SIZE + 1);
            std::io::copy(&mut limited, &mut out)?;
        }

        let db_bytes = std::fs::read(&temp_db_path)?;
        let actual_db_sha = compute_sha256_hex(&db_bytes);
        anyhow::ensure!(
            actual_db_sha == manifest.db_sha256,
            "Database integrity check failed: expected SHA-256 {}, got {}",
            manifest.db_sha256,
            actual_db_sha
        );

        anyhow::ensure!(
            db_bytes.len() as u64 == manifest.db_size_bytes && db_bytes.len() as u64 <= MAX_DB_SIZE,
            "Database decompressed size does not match manifest"
        );

        // Validate every declared member in a private directory. Nothing under
        // the live attachments root is opened during extraction.
        let incoming = temp_dir.path().join("incoming");
        std::fs::create_dir(&incoming)?;
        let mut extracted_count = 0;
        let mut total_extracted_bytes = 0u64;
        let mut destinations = HashSet::new();
        for att in &manifest.attachments {
            let filename = att
                .entry_path
                .strip_prefix("attachments/")
                .filter(|name| {
                    !name.is_empty()
                        && *name != "."
                        && *name != ".."
                        && !name.contains('/')
                        && !name.contains('\\')
                })
                .with_context(|| format!("Invalid attachment entry path: {}", att.entry_path))?;
            anyhow::ensure!(
                destinations.insert(filename.to_string()),
                "Duplicate attachment destination: {}",
                filename
            );
            anyhow::ensure!(
                att.size_bytes <= MAX_ATTACHMENT_SIZE,
                "Attachment '{}' exceeds maximum size",
                att.filename
            );
            let mut member = archive.by_name(&att.entry_path).with_context(|| {
                format!("Bundle is missing declared attachment {}", att.entry_path)
            })?;
            anyhow::ensure!(
                !member.is_dir()
                    && member
                        .unix_mode()
                        .is_none_or(|mode| mode & 0o170000 != 0o120000),
                "Attachment member is not a regular file"
            );
            let mut data =
                Vec::with_capacity((att.size_bytes as usize).min(MAX_PREALLOCATION_BYTES));
            member
                .by_ref()
                .take(MAX_ATTACHMENT_SIZE + 1)
                .read_to_end(&mut data)?;
            anyhow::ensure!(
                data.len() as u64 == att.size_bytes,
                "Attachment '{}' decompressed size mismatch",
                att.filename
            );
            anyhow::ensure!(
                compute_sha256_hex(&data) == att.sha256,
                "Attachment integrity verification failed for '{}' in bundle",
                att.filename
            );
            let mut file = tempfile::NamedTempFile::new_in(&incoming)?;
            file.write_all(&data)?;
            file.as_file().sync_all()?;
            file.persist(incoming.join(filename))?;
            extracted_count += 1;
            total_extracted_bytes += data.len() as u64;
        }

        // Opening/migrating the private database rejects incompatible schemas
        // before any live filesystem publication (restore also validates itself).
        let temp_storage = Storage::open(&temp_db_path)?;
        let restored_count = temp_storage.count_memories(None, None, None)?;
        let mut attachment_swap = AttachmentSwap::publish(temp_dir, &attachments_dir)?;
        let imported = if merge {
            storage
                .merge_vault(&temp_storage, replace)
                .map(|count| (count, "merged".to_string()))
        } else {
            storage
                .restore_from_db_file(&temp_db_path)
                .map(|()| (restored_count, "restore".to_string()))
        };
        let (memories_imported, mode) = match imported {
            Ok(result) => result,
            Err(error) => {
                attachment_swap.rollback().context(
                    "Failed to roll back attachment publication; backup retained in staging",
                )?;
                return Err(error);
            }
        };
        attachment_swap.committed = true;

        Ok(BundleImportResult {
            bundle_file: bundle_path.to_path_buf(),
            memories_imported,
            attachments_extracted: extracted_count,
            total_attachment_bytes: total_extracted_bytes,
            manifest,
            mode,
        })
    }
}
