use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{compute_sha256_hex, LightMemConfig, MemoryRecord, MemoryStatus};
use crate::services::GoogleDriveClient;
use crate::storage::Storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    pub format: String,
    pub last_synced_at: DateTime<Utc>,
    pub vault_db_sha256: String,
    pub total_blobs: usize,
    pub total_blob_bytes: u64,
    pub active_memories_synced: usize,
    pub local_only_memories_exempt: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub target: PathBuf,
    pub blobs_written: usize,
    pub blobs_skipped_existing: usize,
    pub blob_bytes_transferred: u64,
    pub snapshot_path: Option<PathBuf>,
    pub snapshot_bytes: u64,
    pub total_active_memories: usize,
    pub local_only_memories: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusReport {
    pub target: PathBuf,
    pub configured: bool,
    pub target_exists: bool,
    pub blobs_count: usize,
    pub total_blob_bytes: u64,
    pub snapshots_count: usize,
    pub latest_snapshot_date: Option<DateTime<Utc>>,
    pub local_only_memories_count: usize,
    pub syncable_memories_count: usize,
    pub daemon_pid: Option<u32>,
    pub daemon_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullReport {
    pub target: PathBuf,
    pub blobs_found_in_target: usize,
    pub restored_db: bool,
    pub restored_snapshot_path: Option<PathBuf>,
}

/// Configuration options for vault and media synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncOptions {
    pub backup_only: bool,
    pub skip_db: bool,
}

impl SyncOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn backup_only(mut self, val: bool) -> Self {
        self.backup_only = val;
        self
    }

    pub fn skip_db(mut self, val: bool) -> Self {
        self.skip_db = val;
        self
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SyncProgress<'a> {
    Starting {
        target: &'a Path,
    },
    Scanning {
        total_memories: usize,
        local_exempt: usize,
    },
    SyncingBlob {
        current: usize,
        total: usize,
        filename: &'a str,
        sha256: &'a str,
        size_bytes: u64,
        already_exists: bool,
    },
    BlobTransferProgress {
        filename: &'a str,
        bytes_sent: usize,
        total_bytes: usize,
    },
    CreatingSnapshot,
    SnapshotTransferProgress {
        filename: &'a str,
        bytes_sent: usize,
        total_bytes: usize,
    },
    SnapshotCreated {
        filename: &'a str,
        bytes: u64,
    },
    WritingManifest,
    Complete,
}

pub struct SyncService;

impl SyncService {
    /// Perform a push sync to target CAS storage & SQLite vector snapshot
    pub fn push(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        backup_only: bool,
    ) -> Result<SyncReport> {
        Self::push_with_options(storage, config, custom_target, backup_only, false, |_| {})
    }

    /// Perform a push sync with real-time progress callbacks for terminal spinners/loaders
    pub fn push_with_progress<F>(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        backup_only: bool,
        on_progress: F,
    ) -> Result<SyncReport>
    where
        F: FnMut(SyncProgress),
    {
        Self::push_with_options(
            storage,
            config,
            custom_target,
            backup_only,
            false,
            on_progress,
        )
    }

    /// Perform a push sync with options for skipping media or database snapshots
    pub fn push_with_options<F>(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        backup_only: bool,
        skip_db: bool,
        mut on_progress: F,
    ) -> Result<SyncReport>
    where
        F: FnMut(SyncProgress),
    {
        let is_gdrive = match custom_target {
            Some(p) => {
                let s = p.to_string_lossy();
                s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
            }
            None => {
                if let Some(target) = config.effective_sync_target() {
                    let s = target.to_string_lossy();
                    s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
                } else {
                    GoogleDriveClient::is_connected()
                }
            }
        };

        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            return Self::push_to_gdrive(storage, backup_only, skip_db, on_progress);
        }

        let target = custom_target
            .map(Path::to_path_buf)
            .or_else(|| config.effective_sync_target())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No sync target configured. Specify via --target <PATH>, 'lmem config --sync-target <PATH>', or connect Google Drive via 'lmem auth login'"
                )
            })?;

        on_progress(SyncProgress::Starting { target: &target });

        let blobs_dir = target.join("blobs");
        let snapshots_dir = target.join("snapshots");
        fs::create_dir_all(&blobs_dir).with_context(|| {
            format!(
                "Failed to create blobs directory at {}",
                blobs_dir.display()
            )
        })?;
        fs::create_dir_all(&snapshots_dir).with_context(|| {
            format!(
                "Failed to create snapshots directory at {}",
                snapshots_dir.display()
            )
        })?;

        let all_memories = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let mut local_only_memories = 0;
        let mut syncable_memories = Vec::new();

        for mem in all_memories {
            if mem.is_local() {
                local_only_memories += 1;
            } else {
                syncable_memories.push(mem);
            }
        }

        on_progress(SyncProgress::Scanning {
            total_memories: syncable_memories.len(),
            local_exempt: local_only_memories,
        });

        let mut blobs_written = 0;
        let mut blobs_skipped_existing = 0;
        let mut blob_bytes_transferred = 0u64;

        if !backup_only {
            let mut attachments_to_sync = Vec::new();
            for mem in &syncable_memories {
                let attachments_data = storage.get_memory_attachments_data(&mem.id)?;
                attachments_to_sync.extend(attachments_data);
            }
            let total_attachments = attachments_to_sync.len();

            for (idx, (meta, data)) in attachments_to_sync.iter().enumerate() {
                let ext = Path::new(&meta.filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
                let blob_filename = format!("{}.{}", meta.sha256, ext);
                let blob_path = blobs_dir.join(&blob_filename);

                if blob_path.exists() {
                    blobs_skipped_existing += 1;
                    on_progress(SyncProgress::SyncingBlob {
                        current: idx + 1,
                        total: total_attachments,
                        filename: &meta.filename,
                        sha256: &meta.sha256,
                        size_bytes: meta.size_bytes,
                        already_exists: true,
                    });
                    continue;
                }

                // Write to temp file then atomic rename
                let tmp_path =
                    blobs_dir.join(format!("{}.tmp.{}", meta.sha256, uuid::Uuid::new_v4()));
                fs::write(&tmp_path, data).with_context(|| {
                    format!("Failed to write CAS blob to {}", tmp_path.display())
                })?;
                fs::rename(&tmp_path, &blob_path).with_context(|| {
                    format!("Failed to finalize CAS blob at {}", blob_path.display())
                })?;

                blobs_written += 1;
                blob_bytes_transferred += data.len() as u64;
                on_progress(SyncProgress::SyncingBlob {
                    current: idx + 1,
                    total: total_attachments,
                    filename: &meta.filename,
                    sha256: &meta.sha256,
                    size_bytes: meta.size_bytes,
                    already_exists: false,
                });
            }
        }

        let mut snapshot_path = None;
        let mut snapshot_bytes = 0u64;
        let mut vault_db_sha256 = String::new();
        let now = Utc::now();

        if !skip_db {
            on_progress(SyncProgress::CreatingSnapshot);
            let timestamp_str = now.format("%Y%m%d_%H%M%S").to_string();
            let timestamped_db = snapshots_dir.join(format!("vault_{}.db", timestamp_str));
            let latest_db = snapshots_dir.join("vault_latest.db");

            storage.backup_to_file(&timestamped_db)?;
            snapshot_bytes = fs::metadata(&timestamped_db)?.len();
            let _ = fs::copy(&timestamped_db, &latest_db);

            let db_bytes = fs::read(&timestamped_db)?;
            vault_db_sha256 = compute_sha256_hex(&db_bytes);
            on_progress(SyncProgress::SnapshotCreated {
                filename: "vault_latest.db",
                bytes: snapshot_bytes,
            });
            snapshot_path = Some(timestamped_db);
        }

        // Count existing blobs in target
        let total_blobs = fs::read_dir(&blobs_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file() && !e.path().to_string_lossy().contains(".tmp."))
            .count();

        on_progress(SyncProgress::WritingManifest);
        let manifest = SyncManifest {
            format: "lightmem-cas-v1".to_string(),
            last_synced_at: now,
            vault_db_sha256,
            total_blobs,
            total_blob_bytes: blob_bytes_transferred,
            active_memories_synced: syncable_memories.len(),
            local_only_memories_exempt: local_only_memories,
        };

        let manifest_path = target.join("manifest.json");
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_bytes)?;

        on_progress(SyncProgress::Complete);

        Ok(SyncReport {
            target,
            blobs_written,
            blobs_skipped_existing,
            blob_bytes_transferred,
            snapshot_path,
            snapshot_bytes,
            total_active_memories: syncable_memories.len(),
            local_only_memories,
        })
    }

    /// Push CAS blobs and SQLite vector snapshot directly to Google Drive
    pub fn push_to_gdrive<F>(
        storage: &Storage,
        backup_only: bool,
        skip_db: bool,
        mut on_progress: F,
    ) -> Result<SyncReport>
    where
        F: FnMut(SyncProgress),
    {
        let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
            anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
        })?;

        let folders = client.ensure_folders()?;
        let target_display = PathBuf::from(format!("gdrive://LightMem ({})", client.user_email));

        on_progress(SyncProgress::Starting {
            target: &target_display,
        });

        let all_memories = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let mut local_only_memories = 0;
        let mut syncable_memories = Vec::new();

        for mem in all_memories {
            if mem.is_local() {
                local_only_memories += 1;
            } else {
                syncable_memories.push(mem);
            }
        }

        on_progress(SyncProgress::Scanning {
            total_memories: syncable_memories.len(),
            local_exempt: local_only_memories,
        });

        let mut blobs_written = 0;
        let mut blobs_skipped_existing = 0;
        let mut blob_bytes_transferred = 0u64;

        if !backup_only {
            let existing_blobs = client.list_existing_blobs(&folders.blobs_id)?;
            let mut attachments_to_sync = Vec::new();
            for mem in &syncable_memories {
                let attachments_data = storage.get_memory_attachments_data(&mem.id)?;
                attachments_to_sync.extend(attachments_data);
            }
            let total_attachments = attachments_to_sync.len();

            for (idx, (meta, data)) in attachments_to_sync.iter().enumerate() {
                let ext = Path::new(&meta.filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
                let blob_filename = format!("{}.{}", meta.sha256, ext);

                if existing_blobs.contains(&blob_filename) {
                    blobs_skipped_existing += 1;
                    on_progress(SyncProgress::SyncingBlob {
                        current: idx + 1,
                        total: total_attachments,
                        filename: &meta.filename,
                        sha256: &meta.sha256,
                        size_bytes: meta.size_bytes,
                        already_exists: true,
                    });
                    continue;
                }

                client.upload_file_multipart_with_progress(
                    &blob_filename,
                    &folders.blobs_id,
                    &meta.mime_type,
                    data,
                    |sent, total| {
                        on_progress(SyncProgress::BlobTransferProgress {
                            filename: &meta.filename,
                            bytes_sent: sent,
                            total_bytes: total,
                        });
                    },
                )?;

                blobs_written += 1;
                blob_bytes_transferred += data.len() as u64;
                on_progress(SyncProgress::SyncingBlob {
                    current: idx + 1,
                    total: total_attachments,
                    filename: &meta.filename,
                    sha256: &meta.sha256,
                    size_bytes: meta.size_bytes,
                    already_exists: false,
                });
            }
        }

        let mut snapshot_path = None;
        let mut snapshot_bytes = 0u64;
        let mut vault_sha256 = String::new();
        let now = Utc::now();

        if !skip_db {
            on_progress(SyncProgress::CreatingSnapshot);
            let tmp_dir = tempfile::tempdir()?;
            let timestamp_str = now.format("%Y%m%d_%H%M%S").to_string();
            let tmp_db = tmp_dir.path().join(format!("vault_{}.db", timestamp_str));
            storage.backup_to_file(&tmp_db)?;
            snapshot_bytes = fs::metadata(&tmp_db)?.len();

            let db_data = fs::read(&tmp_db)?;
            vault_sha256 = compute_sha256_hex(&db_data);
            let snapshot_filename = format!("vault_{}.db", timestamp_str);

            // Upload or update in-place vault_latest.db with live transfer progress
            client.upload_or_update_file_with_progress(
                "vault_latest.db",
                &folders.snapshots_id,
                "application/x-sqlite3",
                &db_data,
                |sent, total| {
                    on_progress(SyncProgress::SnapshotTransferProgress {
                        filename: "vault_latest.db",
                        bytes_sent: sent,
                        total_bytes: total,
                    });
                },
            )?;

            on_progress(SyncProgress::SnapshotCreated {
                filename: &snapshot_filename,
                bytes: snapshot_bytes,
            });
            snapshot_path = Some(PathBuf::from(&snapshot_filename));
        }

        // Upload manifest
        on_progress(SyncProgress::WritingManifest);
        let manifest = SyncManifest {
            format: "lightmem-cas-v1".to_string(),
            last_synced_at: now,
            vault_db_sha256: vault_sha256,
            total_blobs: blobs_written + blobs_skipped_existing,
            total_blob_bytes: blob_bytes_transferred,
            active_memories_synced: syncable_memories.len(),
            local_only_memories_exempt: local_only_memories,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        client.upload_or_update_file(
            "manifest.json",
            &folders.root_id,
            "application/json",
            &manifest_bytes,
        )?;

        on_progress(SyncProgress::Complete);

        Ok(SyncReport {
            target: target_display,
            blobs_written,
            blobs_skipped_existing,
            blob_bytes_transferred,
            snapshot_path,
            snapshot_bytes,
            total_active_memories: syncable_memories.len(),
            local_only_memories,
        })
    }

    /// Fast synchronous push triggered upon memory creation when auto-sync is enabled
    pub fn sync_memory_inline(
        storage: &Storage,
        config: &LightMemConfig,
        memory: &MemoryRecord,
    ) -> Result<Option<SyncReport>> {
        Self::push_inline_if_enabled(storage, config, memory)
    }

    /// Fast synchronous push triggered upon memory creation when auto-sync is enabled
    pub fn push_inline_if_enabled(
        storage: &Storage,
        config: &LightMemConfig,
        memory: &MemoryRecord,
    ) -> Result<Option<SyncReport>> {
        if memory.is_local() {
            return Ok(None);
        }
        if !config.is_sync_auto_enabled() {
            return Ok(None);
        }
        let report = Self::push(storage, config, None, false)?;
        Ok(Some(report))
    }

    /// Inspect sync status
    pub fn status(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
    ) -> Result<SyncStatusReport> {
        let is_gdrive = match custom_target {
            Some(p) => {
                let s = p.to_string_lossy();
                s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
            }
            None => {
                if let Some(target) = config.effective_sync_target() {
                    let s = target.to_string_lossy();
                    s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
                } else {
                    GoogleDriveClient::is_connected()
                }
            }
        };

        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            return Self::status_gdrive(storage);
        }

        let (target, configured) = match custom_target
            .map(Path::to_path_buf)
            .or_else(|| config.effective_sync_target())
        {
            Some(t) => (t, true),
            None => (
                dirs::home_dir()
                    .map(|h| h.join(".lightmem").join("sync_vault"))
                    .unwrap_or_else(|| PathBuf::from("./sync_vault")),
                false,
            ),
        };

        let target_exists = target.exists();
        let blobs_dir = target.join("blobs");
        let snapshots_dir = target.join("snapshots");

        let mut blobs_count = 0;
        let mut total_blob_bytes = 0u64;
        if blobs_dir.exists() {
            if let Ok(entries) = fs::read_dir(&blobs_dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_file() && !entry.path().to_string_lossy().contains(".tmp.") {
                            blobs_count += 1;
                            total_blob_bytes += meta.len();
                        }
                    }
                }
            }
        }

        let mut snapshots_count = 0;
        let mut latest_snapshot_date = None;
        if snapshots_dir.exists() {
            if let Ok(entries) = fs::read_dir(&snapshots_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("db")
                    {
                        snapshots_count += 1;
                        if let Ok(meta) = entry.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                let dt: DateTime<Utc> = mtime.into();
                                if latest_snapshot_date.map(|prev| dt > prev).unwrap_or(true) {
                                    latest_snapshot_date = Some(dt);
                                }
                            }
                        }
                    }
                }
            }
        }

        let all = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let local_only = all.iter().filter(|m| m.is_local()).count();
        let syncable = all.len().saturating_sub(local_only);

        let daemon_pid = Self::get_daemon_status();
        let daemon_running = daemon_pid.is_some();

        Ok(SyncStatusReport {
            target,
            configured,
            target_exists,
            blobs_count,
            total_blob_bytes,
            snapshots_count,
            latest_snapshot_date,
            local_only_memories_count: local_only,
            syncable_memories_count: syncable,
            daemon_pid,
            daemon_running,
        })
    }

    /// Check Google Drive sync status
    pub fn status_gdrive(storage: &Storage) -> Result<SyncStatusReport> {
        let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
            anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
        })?;
        let folders = client.ensure_folders()?;
        let target_display = PathBuf::from(format!("gdrive://LightMem ({})", client.user_email));

        let blobs = client
            .list_existing_blobs(&folders.blobs_id)
            .unwrap_or_default();
        let all = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let local_only = all.iter().filter(|m| m.is_local()).count();
        let syncable = all.len().saturating_sub(local_only);
        let daemon_pid = Self::get_daemon_status();
        let daemon_running = daemon_pid.is_some();

        Ok(SyncStatusReport {
            target: target_display,
            configured: true,
            target_exists: true,
            blobs_count: blobs.len(),
            total_blob_bytes: 0,
            snapshots_count: 1,
            latest_snapshot_date: None,
            local_only_memories_count: local_only,
            syncable_memories_count: syncable,
            daemon_pid,
            daemon_running,
        })
    }

    /// Pull / restore
    pub fn pull(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        restore_db: bool,
    ) -> Result<SyncPullReport> {
        let is_gdrive = match custom_target {
            Some(p) => {
                let s = p.to_string_lossy();
                s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
            }
            None => {
                if let Some(target) = config.effective_sync_target() {
                    let s = target.to_string_lossy();
                    s == "gdrive" || s == "google-drive" || s == "googledrive" || s == "drive"
                } else {
                    GoogleDriveClient::is_connected()
                }
            }
        };

        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            return Self::pull_gdrive(storage, restore_db);
        }

        let target = custom_target
            .map(Path::to_path_buf)
            .or_else(|| config.effective_sync_target())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No sync target configured. Specify via --target <PATH> or 'lmem config --sync-target <PATH>'"
                )
            })?;

        let blobs_dir = target.join("blobs");
        let snapshots_dir = target.join("snapshots");
        anyhow::ensure!(
            target.exists(),
            "Sync target directory '{}' does not exist",
            target.display()
        );

        let blobs_found = if blobs_dir.exists() {
            fs::read_dir(&blobs_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file() && !e.path().to_string_lossy().contains(".tmp."))
                .count()
        } else {
            0
        };

        let mut restored = false;
        let mut restored_path = None;

        if restore_db {
            let latest_db = snapshots_dir.join("vault_latest.db");
            anyhow::ensure!(
                latest_db.exists(),
                "No vault_latest.db found in '{}' to restore from",
                snapshots_dir.display()
            );
            storage.restore_from_db_file(&latest_db)?;
            restored = true;
            restored_path = Some(latest_db);
        }

        Ok(SyncPullReport {
            target,
            blobs_found_in_target: blobs_found,
            restored_db: restored,
            restored_snapshot_path: restored_path,
        })
    }

    /// Pull / restore latest database snapshot from Google Drive
    pub fn pull_gdrive(storage: &Storage, restore_db: bool) -> Result<SyncPullReport> {
        let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
            anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
        })?;
        let folders = client.ensure_folders()?;
        let target_display = PathBuf::from(format!("gdrive://LightMem ({})", client.user_email));

        let blobs = client
            .list_existing_blobs(&folders.blobs_id)
            .unwrap_or_default();
        let mut restored = false;
        let mut restored_path = None;

        if restore_db {
            let file_id = client
                .find_file("vault_latest.db", &folders.snapshots_id)?
                .ok_or_else(|| {
                    anyhow::anyhow!("No vault_latest.db found in Google Drive snapshots folder")
                })?;
            let db_bytes = client.download_file(&file_id)?;

            let tmp_dir = tempfile::tempdir()?;
            let tmp_db = tmp_dir.path().join("vault_latest_downloaded.db");
            fs::write(&tmp_db, &db_bytes)?;

            storage.restore_from_db_file(&tmp_db)?;
            restored = true;
            restored_path = Some(PathBuf::from("gdrive://snapshots/vault_latest.db"));
        }

        Ok(SyncPullReport {
            target: target_display,
            blobs_found_in_target: blobs.len(),
            restored_db: restored,
            restored_snapshot_path: restored_path,
        })
    }

    /// Path to sync daemon PID file
    pub fn daemon_pid_path() -> PathBuf {
        GoogleDriveClient::config_dir().join("sync_daemon.pid")
    }

    /// Path to sync daemon log file
    pub fn daemon_log_path() -> PathBuf {
        GoogleDriveClient::config_dir().join("sync_daemon.log")
    }

    /// Check if a PID is alive on the current operating system
    pub fn is_pid_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    /// Get current sync daemon PID if running
    pub fn get_daemon_status() -> Option<u32> {
        let p = Self::daemon_pid_path();
        if !p.exists() {
            return None;
        }
        let txt = fs::read_to_string(&p).ok()?;
        let pid: u32 = txt.trim().parse().ok()?;
        if Self::is_pid_alive(pid) {
            Some(pid)
        } else {
            let _ = fs::remove_file(&p);
            None
        }
    }

    /// Stop running sync daemon
    pub fn stop_daemon() -> Result<Option<u32>> {
        let p = Self::daemon_pid_path();
        if !p.exists() {
            return Ok(None);
        }
        let txt = fs::read_to_string(&p)?;
        let pid: u32 = txt.trim().parse()?;
        if Self::is_pid_alive(pid) {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
        }
        let _ = fs::remove_file(&p);
        Ok(Some(pid))
    }

    /// Spawn sync daemon in detached background process
    pub fn spawn_daemon(target: Option<&Path>, options: SyncOptions) -> Result<u32> {
        if let Some(pid) = Self::get_daemon_status() {
            anyhow::bail!("Sync daemon is already running (PID: {})", pid);
        }

        let exe = std::env::current_exe()
            .with_context(|| "Failed to determine current executable path")?;

        let log_path = Self::daemon_log_path();
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open daemon log at {}", log_path.display()))?;

        let mut cmd = std::process::Command::new(exe);
        cmd.arg("sync").arg("--daemon-worker");
        if let Some(t) = target {
            cmd.arg("--target").arg(t);
        }
        if options.backup_only {
            cmd.arg("--skip-media");
        }
        if options.skip_db {
            cmd.arg("--skip-db");
        }

        cmd.stdout(log_file.try_clone()?);
        cmd.stderr(log_file);

        let child = cmd
            .spawn()
            .with_context(|| "Failed to spawn background sync daemon")?;
        let pid = child.id();

        let pid_path = Self::daemon_pid_path();
        fs::write(&pid_path, pid.to_string())?;

        Ok(pid)
    }

    /// Background daemon worker loop that periodically syncs changes
    pub fn run_daemon_worker(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        options: SyncOptions,
    ) -> Result<()> {
        let pid = std::process::id();
        let pid_path = Self::daemon_pid_path();
        fs::write(&pid_path, pid.to_string())?;

        let interval = std::time::Duration::from_secs(300); // 5 minutes

        loop {
            let now = Utc::now();
            eprintln!(
                "[{}] Sync daemon cycle starting...",
                now.format("%Y-%m-%d %H:%M:%S UTC")
            );
            match Self::push_with_options(
                storage,
                config,
                custom_target,
                options.backup_only,
                options.skip_db,
                |_| {},
            ) {
                Ok(rep) => {
                    eprintln!(
                        "[{}] Sync completed: {} blobs written, {} skipped.",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        rep.blobs_written,
                        rep.blobs_skipped_existing
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[{}] Sync error: {:#}",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        e
                    );
                }
            }
            std::thread::sleep(interval);
        }
    }
}
