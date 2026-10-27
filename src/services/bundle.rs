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
        let zip_file = {
            #[cfg(unix)]
            {
                use std::fs::OpenOptions;
                use std::os::unix::fs::OpenOptionsExt;
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&target_path)
                    .with_context(|| format!("Failed to create bundle file at {:?}", target_path))?
            }
            #[cfg(not(unix))]
            {
                File::create(&target_path)
                    .with_context(|| format!("Failed to create bundle file at {:?}", target_path))?
            }
        };

        let mut zip = ZipWriter::new(zip_file);
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
        manifest_file.read_to_string(&mut content)?;

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
        let manifest = Self::inspect_bundle(bundle_path)?;

        let file = File::open(bundle_path)?;
        let mut archive = ZipArchive::new(file)?;

        // 1. Extract and verify memories.db
        let temp_dir = tempfile::tempdir()?;
        let temp_db_path = temp_dir.path().join("memories.db");
        {
            let mut db_file = archive
                .by_name("memories.db")
                .with_context(|| "Bundle is missing memories.db")?;
            let mut out = File::create(&temp_db_path)?;
            std::io::copy(&mut db_file, &mut out)?;
        }

        let db_bytes = std::fs::read(&temp_db_path)?;
        let actual_db_sha = compute_sha256_hex(&db_bytes);
        anyhow::ensure!(
            actual_db_sha == manifest.db_sha256,
            "Database integrity check failed: expected SHA-256 {}, got {}",
            manifest.db_sha256,
            actual_db_sha
        );

        // 2. Extract and verify all video, image, document attachments into destination attachments/ folder
        let attachments_dir = storage
            .path()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("attachments");
        std::fs::create_dir_all(&attachments_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&attachments_dir, std::fs::Permissions::from_mode(0o700));
        }

        let mut extracted_count = 0;
        let mut total_extracted_bytes = 0u64;

        for att in &manifest.attachments {
            if let Ok(mut zip_att) = archive.by_name(&att.entry_path) {
                let mut data = Vec::with_capacity(att.size_bytes as usize);
                zip_att.read_to_end(&mut data)?;
                let actual_sha = compute_sha256_hex(&data);
                anyhow::ensure!(
                    actual_sha == att.sha256,
                    "Attachment integrity verification failed for '{}' in bundle",
                    att.filename
                );

                let safe_filename = Path::new(&att.filename)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .filter(|s| {
                        !s.is_empty() && !s.contains("..") && !s.contains('/') && !s.contains('\\')
                    })
                    .unwrap_or("attachment.bin");

                let target_file = attachments_dir.join(safe_filename);
                #[cfg(unix)]
                {
                    use std::fs::OpenOptions;
                    use std::os::unix::fs::OpenOptionsExt;
                    let mut f = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(&target_file)?;
                    f.write_all(&data)?;
                }
                #[cfg(not(unix))]
                {
                    std::fs::write(&target_file, &data)?;
                }
                extracted_count += 1;
                total_extracted_bytes += data.len() as u64;
            }
        }

        // 3. Database import / restore logic
        let existing_count = storage.count_memories(None, None, None)?;
        let (memories_imported, mode) = if merge {
            let temp_storage = Storage::open(&temp_db_path)?;
            let merged = storage.merge_vault(&temp_storage, replace)?;
            (merged, "merged".to_string())
        } else if replace || existing_count == 0 {
            storage.restore_from_db_file(&temp_db_path)?;
            let restored_count = storage.count_memories(None, None, None)?;
            (restored_count, "restore".to_string())
        } else {
            anyhow::bail!(
                "Target database already contains {} memories. Pass --replace to completely overwrite the vault with this transfer bundle, or --merge to combine them.",
                existing_count
            );
        };

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
