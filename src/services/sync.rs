use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::models::{
    cas_blob_filename, compute_file_sha256_hex, AttachmentMetadata, LightMemConfig, MemoryRecord,
    MemoryStatus, PortableSyncConfig,
};
use crate::services::GoogleDriveClient;
use crate::storage::Storage;

const SYNC_ZIP_ENTRY_NAME: &str = "memories.db";
const SYNC_ZIP_FILENAME: &str = "vault_latest.db.zip";
const SYNC_LEGACY_DB_FILENAME: &str = "vault_latest.db";
const MAX_SYNC_DB_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
const SYNC_LOCK_STALE_SECS: i64 = 120;
const SYNC_LOCK_WAIT_MS: u64 = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    pub format: String,
    pub last_synced_at: DateTime<Utc>,
    pub vault_db_sha256: String,
    pub total_blobs: usize,
    pub total_blob_bytes: u64,
    pub active_memories_synced: usize,
    pub local_only_memories_exempt: usize,
    #[serde(default)]
    pub snapshot_filename: Option<String>,
    #[serde(default)]
    pub writer_device_id: Option<String>,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub embedding_identity: Option<String>,
    #[serde(default)]
    pub portable_config: Option<PortableSyncConfig>,
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
    #[serde(default)]
    pub pulled_first: bool,
    #[serde(default)]
    pub merged_memories: usize,
    #[serde(default)]
    pub updated_memories: usize,
    #[serde(default)]
    pub deleted_memories: usize,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub snapshot_uncompressed_bytes: u64,
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
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub remote_manifest_exists: bool,
    #[serde(default)]
    pub remote_sha256: Option<String>,
    #[serde(default)]
    pub local_last_synced_sha256: Option<String>,
    #[serde(default)]
    pub remote_ahead: bool,
    #[serde(default)]
    pub local_dirty: bool,
    #[serde(default)]
    pub in_sync: bool,
    #[serde(default)]
    pub remote_check_skipped_offline: bool,
    #[serde(default)]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub remote_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullReport {
    pub target: PathBuf,
    pub blobs_found_in_target: usize,
    pub restored_db: bool,
    pub restored_snapshot_path: Option<PathBuf>,
    #[serde(default)]
    pub skipped_up_to_date: bool,
    #[serde(default)]
    pub merged_memories: usize,
    #[serde(default)]
    pub updated_memories: usize,
    #[serde(default)]
    pub deleted_memories: usize,
    #[serde(default)]
    pub local_kept_newer: usize,
    #[serde(default)]
    pub remote_portable_config: Option<PortableSyncConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct InlinePullOutcome {
    pub pulled: bool,
    pub merged_memories: usize,
    pub updated_memories: usize,
    pub deleted_memories: usize,
    pub remote_portable_config: Option<PortableSyncConfig>,
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

fn normalize_path_key(path: &Path) -> PathBuf {
    if path == Path::new(":memory:") {
        return path.to_path_buf();
    }
    if let Ok(canon) = path.canonicalize() {
        return canon;
    }
    let mut existing = path.to_path_buf();
    let mut suffix = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            suffix.push(name.to_os_string());
        } else {
            break;
        }
        if !existing.pop() {
            break;
        }
    }
    if existing.as_os_str().is_empty() {
        existing = PathBuf::from(".");
    }
    let mut resolved = existing.canonicalize().unwrap_or(existing);
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    resolved
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    pub device_id: String,
    pub last_synced_target: Option<String>,
    pub last_synced_sha256: Option<String>,
    pub last_synced_revision: i64,
    pub last_synced_sequence: u64,
    pub last_synced_at: Option<DateTime<Utc>>,
}

impl SyncState {
    pub fn path_for_db(db_path: &Path) -> Option<PathBuf> {
        if db_path == Path::new(":memory:") {
            return None;
        }
        let normalized = normalize_path_key(db_path);
        let parent = normalized
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let key_str = normalized.to_string_lossy();
        let mut hash = 0xcbf29ce484222325u64;
        for b in key_str.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Some(parent.join(format!(".lightmem_sync_state_{:016x}.json", hash)))
    }

    pub fn load_for_db(db_path: &Path) -> Self {
        let Some(path) = Self::path_for_db(db_path) else {
            return Self {
                device_id: resolve_or_create_device_id(),
                ..Default::default()
            };
        };
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(mut parsed) = serde_json::from_str::<SyncState>(&raw) {
                if parsed.device_id.trim().is_empty() {
                    parsed.device_id = resolve_or_create_device_id();
                }
                return parsed;
            }
        }
        Self {
            device_id: resolve_or_create_device_id(),
            ..Default::default()
        }
    }

    pub fn save_for_db(&self, db_path: &Path) -> Result<()> {
        let Some(path) = Self::path_for_db(db_path) else {
            return Ok(());
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let tmp_path = path.with_extension(format!("json.tmp.{}", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(&tmp_path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }
        if fs::rename(&tmp_path, &path).is_err() {
            let _ = fs::remove_file(&tmp_path);
            fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        }
        Ok(())
    }

    fn effective_cursor_for_target(&self, target_key: &str, current_rev: i64) -> Option<String> {
        if self.last_synced_target.as_deref() != Some(target_key) {
            return None;
        }
        // If local DB was deleted/recreated externally (revision reset to 0 while cursor > 0), invalidate cursor.
        if current_rev == 0 && self.last_synced_revision > 0 {
            return None;
        }
        self.last_synced_sha256.clone()
    }

    fn is_local_dirty(&self, target_key: &str, current_rev: i64, has_local_syncable: bool) -> bool {
        let target_matches = self.last_synced_target.as_deref() == Some(target_key)
            && self.last_synced_sha256.is_some()
            && !(current_rev == 0 && self.last_synced_revision > 0);
        if target_matches {
            current_rev != self.last_synced_revision
        } else {
            has_local_syncable
        }
    }
}

struct SyncLock {
    path: Option<PathBuf>,
}

impl SyncLock {
    fn acquire(db_path: &Path) -> Result<Self> {
        if db_path == Path::new(":memory:") {
            return Ok(Self { path: None });
        }
        let lock_path = PathBuf::from(format!("{}.sync.lock", db_path.display()));
        if let Some(parent) = lock_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }

        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(SYNC_LOCK_WAIT_MS);
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut f) => {
                    let payload = format!("{}\n{}\n", std::process::id(), Utc::now().to_rfc3339());
                    let _ = f.write_all(payload.as_bytes());
                    let _ = f.flush();
                    return Ok(Self {
                        path: Some(lock_path),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Self::is_stale_lock(&lock_path) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if std::time::Instant::now() >= deadline {
                        anyhow::bail!(
                            "Another LightMem sync process holds {}; try again shortly",
                            lock_path.display()
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("Failed to acquire sync lock at {}", lock_path.display())
                    });
                }
            }
        }
    }

    fn is_stale_lock(lock_path: &Path) -> bool {
        let Ok(content) = fs::read_to_string(lock_path) else {
            return fs::metadata(lock_path)
                .and_then(|m| m.modified())
                .map(|mt| {
                    mt.elapsed()
                        .map(|d| d.as_secs() as i64 > SYNC_LOCK_STALE_SECS)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
        };
        let mut lines = content.lines();
        let pid_opt = lines.next().and_then(|l| l.trim().parse::<u32>().ok());
        let ts_opt = lines
            .next()
            .and_then(|l| DateTime::parse_from_rfc3339(l.trim()).ok())
            .map(|dt| dt.with_timezone(&Utc));

        if let Some(pid) = pid_opt {
            #[cfg(unix)]
            if pid != std::process::id() && !SyncService::is_pid_alive(pid) {
                return true;
            }
        }
        if let Some(ts) = ts_opt {
            if Utc::now().signed_duration_since(ts).num_seconds() > SYNC_LOCK_STALE_SECS {
                return true;
            }
        }
        false
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        if let Some(ref p) = self.path {
            let _ = fs::remove_file(p);
        }
    }
}

fn resolve_or_create_device_id() -> String {
    let id_path = GoogleDriveClient::config_dir().join("device_id");
    if let Ok(raw) = fs::read_to_string(&id_path) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "device".to_string())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect::<String>();
    let prefix = if host.is_empty() { "device" } else { &host };
    let new_id = format!("{}-{}", prefix, &uuid::Uuid::new_v4().to_string()[..8]);
    if let Some(parent) = id_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if fs::create_dir_all(parent).is_ok() {
            let _ = fs::write(&id_path, &new_id);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&id_path, fs::Permissions::from_mode(0o600));
            }
        }
    }
    new_id
}

/// Stream-compress a SQLite `.db` file to a `.db.zip` archive containing `"memories.db"`.
pub(crate) fn compress_db_to_zip_streaming(
    src_db_path: &Path,
    zip_dest_path: &Path,
) -> Result<u64> {
    if let Some(parent) = zip_dest_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let raw_len = fs::metadata(src_db_path)
        .with_context(|| format!("Failed to stat SQLite snapshot {}", src_db_path.display()))?
        .len();

    let tmp_zip = zip_dest_path.with_extension(format!("zip.tmp.{}", uuid::Uuid::new_v4()));
    {
        let out_file = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&tmp_zip)?
            }
            #[cfg(not(unix))]
            {
                File::create(&tmp_zip)?
            }
        };
        let mut zip = ZipWriter::new(BufWriter::with_capacity(64 * 1024, out_file));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(raw_len > u32::MAX as u64);
        zip.start_file(SYNC_ZIP_ENTRY_NAME, options)?;
        let src_file = File::open(src_db_path).with_context(|| {
            format!(
                "Failed to open SQLite snapshot for compression: {}",
                src_db_path.display()
            )
        })?;
        let mut reader = BufReader::with_capacity(64 * 1024, src_file);
        std::io::copy(&mut reader, &mut zip)?;
        let mut buf_writer = zip.finish()?;
        buf_writer.flush()?;
    }

    if fs::rename(&tmp_zip, zip_dest_path).is_err() {
        fs::copy(&tmp_zip, zip_dest_path)?;
        let _ = fs::remove_file(&tmp_zip);
    }
    Ok(fs::metadata(zip_dest_path)?.len())
}

/// Stream-extract `"memories.db"` (or first `.db` entry) from a `.db.zip` archive to `db_dest_path`.
pub(crate) fn extract_db_from_zip_streaming(zip_path: &Path, db_dest_path: &Path) -> Result<u64> {
    if let Some(parent) = db_dest_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let file = File::open(zip_path)
        .with_context(|| format!("Failed to open zipped snapshot {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(BufReader::with_capacity(64 * 1024, file))
        .with_context(|| format!("Invalid ZIP snapshot archive {}", zip_path.display()))?;

    let mut exact_index = None;
    let mut fallback_index = None;
    for i in 0..archive.len() {
        if let Ok(e) = archive.by_index(i) {
            if !e.is_file() {
                continue;
            }
            if let Ok(name) = e.name() {
                if name.as_ref() == SYNC_ZIP_ENTRY_NAME {
                    exact_index = Some(i);
                    break;
                }
                if fallback_index.is_none() && name.ends_with(".db") {
                    fallback_index = Some(i);
                }
            }
        }
    }
    let entry_index = exact_index.or(fallback_index).ok_or_else(|| {
        anyhow::anyhow!(
            "Compressed snapshot {} does not contain a SQLite .db entry",
            zip_path.display()
        )
    })?;

    let mut entry = archive.by_index(entry_index)?;
    anyhow::ensure!(
        entry.size() <= MAX_SYNC_DB_BYTES,
        "Compressed snapshot DB entry exceeds maximum size limit ({} bytes)",
        entry.size()
    );

    let out_file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(db_dest_path)?
        }
        #[cfg(not(unix))]
        {
            File::create(db_dest_path)?
        }
    };
    let mut writer = BufWriter::with_capacity(64 * 1024, out_file);
    let mut limited = (&mut entry).take(MAX_SYNC_DB_BYTES + 1);
    let written = std::io::copy(&mut limited, &mut writer)?;
    anyhow::ensure!(
        written <= MAX_SYNC_DB_BYTES,
        "Compressed snapshot DB stream exceeded {} bytes",
        MAX_SYNC_DB_BYTES
    );
    writer.flush()?;
    Ok(written)
}

fn is_gdrive_spec(raw: &str) -> bool {
    let lower = raw.trim().to_ascii_lowercase();
    lower == "gdrive" || lower == "google-drive" || lower == "googledrive" || lower == "drive"
}

fn local_target_key(target: &Path) -> String {
    normalize_path_key(target).to_string_lossy().into_owned()
}

pub struct SyncService;

impl SyncService {
    fn resolve_target_is_gdrive(config: &LightMemConfig, custom_target: Option<&Path>) -> bool {
        match custom_target {
            Some(p) => is_gdrive_spec(&p.to_string_lossy()),
            None => {
                if let Some(target) = config.effective_sync_target() {
                    is_gdrive_spec(&target.to_string_lossy())
                } else {
                    GoogleDriveClient::is_connected()
                }
            }
        }
    }

    fn parse_manifest_bytes(bytes: &[u8]) -> Option<SyncManifest> {
        serde_json::from_slice::<SyncManifest>(bytes).ok()
    }

    fn read_local_manifest(target: &Path) -> Option<SyncManifest> {
        let path = target.join("manifest.json");
        let bytes = fs::read(path).ok()?;
        Self::parse_manifest_bytes(&bytes)
    }

    fn compute_sync_flags(
        storage: &Storage,
        target_key: &str,
        remote_manifest: Option<&SyncManifest>,
        remote_snapshot_exists: bool,
        syncable_count: usize,
    ) -> Result<(bool, bool, bool, Option<String>)> {
        let current_rev = storage.storage_revision()?;
        let tombstones = storage.tombstone_count()?;
        let has_local_syncable = syncable_count > 0 || tombstones > 0;
        let state = SyncState::load_for_db(storage.path());
        let local_last_sha = state.effective_cursor_for_target(target_key, current_rev);
        let local_dirty = state.is_local_dirty(target_key, current_rev, has_local_syncable);

        let remote_ahead = if let Some(m) = remote_manifest {
            if !m.vault_db_sha256.is_empty() {
                local_last_sha.as_deref() != Some(m.vault_db_sha256.as_str())
            } else {
                remote_snapshot_exists && local_last_sha.is_none()
            }
        } else {
            remote_snapshot_exists && local_last_sha.is_none()
        };

        let in_sync =
            !remote_ahead && !local_dirty && (remote_manifest.is_some() || !has_local_syncable);

        Ok((remote_ahead, local_dirty, in_sync, local_last_sha))
    }

    /// Perform a push sync to target CAS storage & compressed SQLite vector snapshot
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
        on_progress: F,
    ) -> Result<SyncReport>
    where
        F: FnMut(SyncProgress),
    {
        if config.is_offline() {
            anyhow::bail!(
                "LightMem sync is currently OFFLINE. Run 'lmem sync --online' (or unset LIGHTMEM_OFFLINE) to resume cloud sync."
            );
        }

        let _lock = SyncLock::acquire(storage.path())?;
        Self::push_with_options_locked(
            storage,
            config,
            custom_target,
            backup_only,
            skip_db,
            on_progress,
        )
    }

    fn push_with_options_locked<F>(
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
        let is_gdrive = Self::resolve_target_is_gdrive(config, custom_target);

        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            return Self::push_to_gdrive_locked(storage, config, backup_only, skip_db, on_progress);
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

        let target_key = local_target_key(&target);
        let remote_manifest = Self::read_local_manifest(&target);
        let remote_snapshot_exists = snapshots_dir.join(SYNC_ZIP_FILENAME).exists()
            || snapshots_dir.join(SYNC_LEGACY_DB_FILENAME).exists();

        // Pre-push safety: if remote changed since our last sync, pull and merge non-destructively before pushing!
        let mut pulled_first = false;
        let mut merged_memories = 0usize;
        let mut updated_memories = 0usize;
        let mut deleted_memories = 0usize;

        if !skip_db && (remote_manifest.is_some() || remote_snapshot_exists) {
            let syncable_now = storage
                .list_memories(None, Some(MemoryStatus::Active), None, 0)?
                .into_iter()
                .filter(|m| !m.is_local())
                .count();
            let (remote_ahead, _, _, _) = Self::compute_sync_flags(
                storage,
                &target_key,
                remote_manifest.as_ref(),
                remote_snapshot_exists,
                syncable_now,
            )?;
            if remote_ahead {
                let pull_rep = Self::pull_local_locked(storage, &target, false)?;
                pulled_first = !pull_rep.skipped_up_to_date;
                merged_memories = pull_rep.merged_memories;
                updated_memories = pull_rep.updated_memories;
                deleted_memories = pull_rep.deleted_memories;
            }
        }

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
        let mut synced_blob_filenames: HashSet<String> = HashSet::new();

        if !backup_only {
            let all_attachments: Vec<&AttachmentMetadata> = syncable_memories
                .iter()
                .flat_map(|m| m.attachments.iter())
                .collect();
            let total_attachments = all_attachments.len();

            for (idx, meta) in all_attachments.iter().enumerate() {
                let blob_filename = cas_blob_filename(&meta.sha256, &meta.filename);
                let blob_path = blobs_dir.join(&blob_filename);

                let already_cached = blob_path.exists()
                    && fs::metadata(&blob_path)
                        .map(|m| m.len() == meta.size_bytes)
                        .unwrap_or(false);

                if already_cached {
                    blobs_skipped_existing += 1;
                    synced_blob_filenames.insert(blob_filename);
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

                let (_, data) =
                    storage
                        .get_attachment_data_by_id(&meta.id)?
                        .with_context(|| {
                            format!(
                                "Attachment '{}' ({}) missing from vault during CAS sync",
                                meta.filename, meta.id
                            )
                        })?;

                let tmp_path =
                    blobs_dir.join(format!("{}.tmp.{}", meta.sha256, uuid::Uuid::new_v4()));
                fs::write(&tmp_path, &data).with_context(|| {
                    format!("Failed to write CAS blob to {}", tmp_path.display())
                })?;
                fs::rename(&tmp_path, &blob_path).with_context(|| {
                    format!("Failed to finalize CAS blob at {}", blob_path.display())
                })?;

                blobs_written += 1;
                blob_bytes_transferred += data.len() as u64;
                drop(data);
                synced_blob_filenames.insert(blob_filename);

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
        let mut snapshot_uncompressed_bytes = 0u64;
        let mut vault_db_sha256 = String::new();
        let now = Utc::now();

        if !skip_db {
            on_progress(SyncProgress::CreatingSnapshot);
            let tmp_dir = tempfile::tempdir()?;
            let raw_db = tmp_dir.path().join("memories.db");
            if backup_only {
                storage.backup_to_file(&raw_db)?;
            } else {
                storage.backup_slim_snapshot_to_file(&raw_db, &synced_blob_filenames)?;
            }
            snapshot_uncompressed_bytes = fs::metadata(&raw_db)?.len();
            vault_db_sha256 = compute_file_sha256_hex(&raw_db)?;

            let timestamp_str = now.format("%Y%m%d_%H%M%S").to_string();
            let timestamped_zip = snapshots_dir.join(format!("vault_{}.db.zip", timestamp_str));
            let latest_zip = snapshots_dir.join(SYNC_ZIP_FILENAME);

            snapshot_bytes = compress_db_to_zip_streaming(&raw_db, &timestamped_zip)?;

            let latest_tmp = snapshots_dir.join(format!(
                "{}.tmp.{}",
                SYNC_ZIP_FILENAME,
                uuid::Uuid::new_v4()
            ));
            if fs::copy(&timestamped_zip, &latest_tmp).is_ok() {
                if fs::rename(&latest_tmp, &latest_zip).is_err() {
                    let _ = fs::remove_file(&latest_tmp);
                    let _ = fs::copy(&timestamped_zip, &latest_zip);
                }
            } else {
                let _ = fs::copy(&timestamped_zip, &latest_zip);
            }

            on_progress(SyncProgress::SnapshotCreated {
                filename: SYNC_ZIP_FILENAME,
                bytes: snapshot_bytes,
            });
            snapshot_path = Some(timestamped_zip);
        }

        let total_blobs = fs::read_dir(&blobs_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file() && !e.path().to_string_lossy().contains(".tmp."))
            .count();

        let mut state = SyncState::load_for_db(storage.path());
        let prev_remote_seq = remote_manifest.as_ref().map(|m| m.sequence).unwrap_or(0);
        let new_sequence = prev_remote_seq.max(state.last_synced_sequence) + 1;

        on_progress(SyncProgress::WritingManifest);
        let manifest = SyncManifest {
            format: "lightmem-cas-v2".to_string(),
            last_synced_at: now,
            vault_db_sha256: vault_db_sha256.clone(),
            total_blobs,
            total_blob_bytes: blob_bytes_transferred,
            active_memories_synced: syncable_memories.len(),
            local_only_memories_exempt: local_only_memories,
            snapshot_filename: if skip_db {
                None
            } else {
                Some(SYNC_ZIP_FILENAME.to_string())
            },
            writer_device_id: Some(state.device_id.clone()),
            sequence: new_sequence,
            embedding_identity: config.portable_embedding_identity().ok(),
            portable_config: Some(config.portable_sync_config()),
        };

        let manifest_path = target.join("manifest.json");
        let manifest_tmp = target.join(format!("manifest.json.tmp.{}", uuid::Uuid::new_v4()));
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(&manifest_tmp, &manifest_bytes)?;
        if fs::rename(&manifest_tmp, &manifest_path).is_err() {
            let _ = fs::remove_file(&manifest_tmp);
            fs::write(&manifest_path, &manifest_bytes)?;
        }

        state.last_synced_target = Some(target_key);
        if !skip_db {
            state.last_synced_sha256 = Some(vault_db_sha256);
            state.last_synced_revision = storage.storage_revision()?;
        }
        state.last_synced_sequence = new_sequence;
        state.last_synced_at = Some(now);
        let _ = state.save_for_db(storage.path());

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
            pulled_first,
            merged_memories,
            updated_memories,
            deleted_memories,
            sequence: new_sequence,
            snapshot_uncompressed_bytes,
        })
    }

    /// Push CAS blobs and compressed SQLite vector snapshot directly to Google Drive
    pub fn push_to_gdrive<F>(
        storage: &Storage,
        backup_only: bool,
        skip_db: bool,
        on_progress: F,
    ) -> Result<SyncReport>
    where
        F: FnMut(SyncProgress),
    {
        let cfg = LightMemConfig::load();
        if cfg.is_offline() {
            anyhow::bail!(
                "LightMem sync is currently OFFLINE. Run 'lmem sync --online' (or unset LIGHTMEM_OFFLINE) to resume cloud sync."
            );
        }
        let _lock = SyncLock::acquire(storage.path())?;
        Self::push_to_gdrive_locked(storage, &cfg, backup_only, skip_db, on_progress)
    }

    fn push_to_gdrive_locked<F>(
        storage: &Storage,
        config: &LightMemConfig,
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

        let remote_manifest = client
            .download_optional_file_by_name("manifest.json", &folders.root_id)?
            .and_then(|bytes| Self::parse_manifest_bytes(&bytes));

        let mut pulled_first = false;
        let mut merged_memories = 0usize;
        let mut updated_memories = 0usize;
        let mut deleted_memories = 0usize;

        if !skip_db && remote_manifest.is_some() {
            let syncable_now = storage
                .list_memories(None, Some(MemoryStatus::Active), None, 0)?
                .into_iter()
                .filter(|m| !m.is_local())
                .count();
            let (remote_ahead, _, _, _) = Self::compute_sync_flags(
                storage,
                "gdrive",
                remote_manifest.as_ref(),
                true,
                syncable_now,
            )?;
            if remote_ahead {
                let pull_rep = Self::pull_gdrive_locked(storage, &client, &folders, false)?;
                pulled_first = !pull_rep.skipped_up_to_date;
                merged_memories = pull_rep.merged_memories;
                updated_memories = pull_rep.updated_memories;
                deleted_memories = pull_rep.deleted_memories;
            }
        }

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
        let mut synced_blob_filenames: HashSet<String> = HashSet::new();

        if !backup_only {
            let mut existing_blobs = client.list_existing_blobs(&folders.blobs_id)?;
            let all_attachments: Vec<&AttachmentMetadata> = syncable_memories
                .iter()
                .flat_map(|m| m.attachments.iter())
                .collect();
            let total_attachments = all_attachments.len();

            for (idx, meta) in all_attachments.iter().enumerate() {
                let blob_filename = cas_blob_filename(&meta.sha256, &meta.filename);

                if existing_blobs.contains(&blob_filename) {
                    blobs_skipped_existing += 1;
                    synced_blob_filenames.insert(blob_filename);
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

                let (_, data) =
                    storage
                        .get_attachment_data_by_id(&meta.id)?
                        .with_context(|| {
                            format!(
                                "Attachment '{}' ({}) missing from vault during Drive sync",
                                meta.filename, meta.id
                            )
                        })?;

                client.upload_file_multipart_with_progress(
                    &blob_filename,
                    &folders.blobs_id,
                    &meta.mime_type,
                    &data,
                    |sent, total| {
                        on_progress(SyncProgress::BlobTransferProgress {
                            filename: &meta.filename,
                            bytes_sent: sent,
                            total_bytes: total,
                        });
                    },
                )?;

                let uploaded_bytes = data.len() as u64;
                drop(data);

                existing_blobs.insert(blob_filename.clone());
                synced_blob_filenames.insert(blob_filename);
                blobs_written += 1;
                blob_bytes_transferred += uploaded_bytes;
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
        let mut snapshot_uncompressed_bytes = 0u64;
        let mut vault_sha256 = String::new();
        let now = Utc::now();

        if !skip_db {
            on_progress(SyncProgress::CreatingSnapshot);
            let tmp_dir = tempfile::tempdir()?;
            let tmp_db = tmp_dir.path().join("memories.db");
            let tmp_zip = tmp_dir.path().join(SYNC_ZIP_FILENAME);
            if backup_only {
                storage.backup_to_file(&tmp_db)?;
            } else {
                storage.backup_slim_snapshot_to_file(&tmp_db, &synced_blob_filenames)?;
            }
            snapshot_uncompressed_bytes = fs::metadata(&tmp_db)?.len();
            vault_sha256 = compute_file_sha256_hex(&tmp_db)?;
            snapshot_bytes = compress_db_to_zip_streaming(&tmp_db, &tmp_zip)?;
            let timestamp_str = now.format("%Y%m%d_%H%M%S").to_string();
            let snapshot_display_name = format!("vault_{}.db.zip", timestamp_str);

            client.upload_or_update_file_from_path_with_progress(
                SYNC_ZIP_FILENAME,
                &folders.snapshots_id,
                "application/zip",
                &tmp_zip,
                |sent, total| {
                    on_progress(SyncProgress::SnapshotTransferProgress {
                        filename: SYNC_ZIP_FILENAME,
                        bytes_sent: sent,
                        total_bytes: total,
                    });
                },
            )?;

            on_progress(SyncProgress::SnapshotCreated {
                filename: &snapshot_display_name,
                bytes: snapshot_bytes,
            });
            snapshot_path = Some(PathBuf::from(&snapshot_display_name));
        }

        let mut state = SyncState::load_for_db(storage.path());
        let prev_remote_seq = remote_manifest.as_ref().map(|m| m.sequence).unwrap_or(0);
        let new_sequence = prev_remote_seq.max(state.last_synced_sequence) + 1;

        on_progress(SyncProgress::WritingManifest);
        let manifest = SyncManifest {
            format: "lightmem-cas-v2".to_string(),
            last_synced_at: now,
            vault_db_sha256: vault_sha256.clone(),
            total_blobs: blobs_written + blobs_skipped_existing,
            total_blob_bytes: blob_bytes_transferred,
            active_memories_synced: syncable_memories.len(),
            local_only_memories_exempt: local_only_memories,
            snapshot_filename: if skip_db {
                None
            } else {
                Some(SYNC_ZIP_FILENAME.to_string())
            },
            writer_device_id: Some(state.device_id.clone()),
            sequence: new_sequence,
            embedding_identity: config.portable_embedding_identity().ok(),
            portable_config: Some(config.portable_sync_config()),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        client.upload_or_update_file(
            "manifest.json",
            &folders.root_id,
            "application/json",
            &manifest_bytes,
        )?;

        state.last_synced_target = Some("gdrive".to_string());
        if !skip_db {
            state.last_synced_sha256 = Some(vault_sha256);
            state.last_synced_revision = storage.storage_revision()?;
        }
        state.last_synced_sequence = new_sequence;
        state.last_synced_at = Some(now);
        let _ = state.save_for_db(storage.path());

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
            pulled_first,
            merged_memories,
            updated_memories,
            deleted_memories,
            sequence: new_sequence,
            snapshot_uncompressed_bytes,
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
        if config.is_offline() || !config.is_sync_auto_enabled() {
            return Ok(None);
        }
        let report = Self::push(storage, config, None, false)?;
        Ok(Some(report))
    }

    /// Inline pre-write sync hook: pulls and merges remote updates when remote is ahead and auto-sync applies.
    pub fn pull_if_remote_ahead(
        storage: &Storage,
        config: &LightMemConfig,
        is_global_default_db: bool,
        explicit_db_or_connection: bool,
    ) -> Result<InlinePullOutcome> {
        let gdrive_connected = GoogleDriveClient::is_connected();
        if !config.should_auto_sync(
            gdrive_connected,
            is_global_default_db,
            explicit_db_or_connection,
        ) {
            return Ok(InlinePullOutcome::default());
        }

        let is_gdrive = Self::resolve_target_is_gdrive(config, None);
        if is_gdrive {
            if !gdrive_connected {
                return Ok(InlinePullOutcome::default());
            }
            let _lock = SyncLock::acquire(storage.path())?;
            let client = match GoogleDriveClient::try_load()? {
                Some(c) => c,
                None => return Ok(InlinePullOutcome::default()),
            };
            let folders = client.ensure_folders()?;
            let rep = Self::pull_gdrive_locked(storage, &client, &folders, false)?;
            return Ok(InlinePullOutcome {
                pulled: !rep.skipped_up_to_date,
                merged_memories: rep.merged_memories,
                updated_memories: rep.updated_memories,
                deleted_memories: rep.deleted_memories,
                remote_portable_config: rep.remote_portable_config,
            });
        }

        let Some(target) = config.effective_sync_target() else {
            return Ok(InlinePullOutcome::default());
        };
        if !target.exists() {
            return Ok(InlinePullOutcome::default());
        }
        let snapshots_dir = target.join("snapshots");
        let manifest_exists = target.join("manifest.json").exists();
        let snap_exists = snapshots_dir.join(SYNC_ZIP_FILENAME).exists()
            || snapshots_dir.join(SYNC_LEGACY_DB_FILENAME).exists();
        if !manifest_exists && !snap_exists {
            return Ok(InlinePullOutcome::default());
        }

        let _lock = SyncLock::acquire(storage.path())?;
        let rep = Self::pull_local_locked(storage, &target, false)?;
        Ok(InlinePullOutcome {
            pulled: !rep.skipped_up_to_date,
            merged_memories: rep.merged_memories,
            updated_memories: rep.updated_memories,
            deleted_memories: rep.deleted_memories,
            remote_portable_config: rep.remote_portable_config,
        })
    }

    /// Inline post-write sync hook for vault mutations (remember, forget, link, dedup, conflicts, etc.)
    pub fn push_inline_if_enabled_for_vault(
        storage: &Storage,
        config: &LightMemConfig,
        is_local_write: bool,
        is_global_default_db: bool,
        explicit_db_or_connection: bool,
    ) -> Result<Option<SyncReport>> {
        if is_local_write {
            return Ok(None);
        }
        if !config.should_auto_sync(
            GoogleDriveClient::is_connected(),
            is_global_default_db,
            explicit_db_or_connection,
        ) {
            return Ok(None);
        }
        let report = Self::push(storage, config, None, false)?;
        Ok(Some(report))
    }

    /// Inspect sync status (offline-safe: skips remote network calls when `sync_offline` / `LIGHTMEM_OFFLINE` is active)
    pub fn status(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
    ) -> Result<SyncStatusReport> {
        let offline = config.is_offline();
        let is_gdrive = Self::resolve_target_is_gdrive(config, custom_target);

        let all = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let local_only = all.iter().filter(|m| m.is_local()).count();
        let syncable = all.len().saturating_sub(local_only);
        let current_rev = storage.storage_revision()?;
        let tombstones = storage.tombstone_count()?;
        let has_local_syncable = syncable > 0 || tombstones > 0;
        let state = SyncState::load_for_db(storage.path());
        let daemon_pid = Self::get_daemon_status();
        let daemon_running = daemon_pid.is_some();

        if offline {
            let (target, configured, target_key) = if is_gdrive {
                let auth = GoogleDriveClient::get_status().ok();
                let connected = auth.as_ref().map(|a| a.connected).unwrap_or(false);
                let email = auth
                    .and_then(|a| a.user_email)
                    .unwrap_or_else(|| "offline".to_string());
                (
                    PathBuf::from(format!("gdrive://LightMem ({})", email)),
                    connected,
                    "gdrive".to_string(),
                )
            } else {
                let (t, c) = match custom_target
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
                let key = local_target_key(&t);
                (t, c, key)
            };

            let local_last_sha = state.effective_cursor_for_target(&target_key, current_rev);
            let local_dirty = state.is_local_dirty(&target_key, current_rev, has_local_syncable);

            return Ok(SyncStatusReport {
                target,
                configured,
                target_exists: !is_gdrive && target_key_exists(&target_key),
                blobs_count: 0,
                total_blob_bytes: 0,
                snapshots_count: 0,
                latest_snapshot_date: None,
                local_only_memories_count: local_only,
                syncable_memories_count: syncable,
                daemon_pid,
                daemon_running,
                offline: true,
                remote_manifest_exists: false,
                remote_sha256: None,
                local_last_synced_sha256: local_last_sha,
                remote_ahead: false,
                local_dirty,
                in_sync: false,
                remote_check_skipped_offline: true,
                last_synced_at: state.last_synced_at,
                remote_sequence: state.last_synced_sequence,
            });
        }

        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            return Self::status_gdrive_with_config(storage, config);
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
        let mut remote_snapshot_exists = false;
        if snapshots_dir.exists() {
            if let Ok(entries) = fs::read_dir(&snapshots_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if path.is_file()
                        && !name.contains(".tmp.")
                        && (name.ends_with(".db") || name.ends_with(".db.zip"))
                    {
                        snapshots_count += 1;
                        if name == SYNC_ZIP_FILENAME || name == SYNC_LEGACY_DB_FILENAME {
                            remote_snapshot_exists = true;
                        }
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

        let target_key = local_target_key(&target);
        let remote_manifest = Self::read_local_manifest(&target);
        let (remote_ahead, local_dirty, in_sync, local_last_synced_sha256) =
            Self::compute_sync_flags(
                storage,
                &target_key,
                remote_manifest.as_ref(),
                remote_snapshot_exists,
                syncable,
            )?;

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
            offline: false,
            remote_manifest_exists: remote_manifest.is_some(),
            remote_sha256: remote_manifest.as_ref().map(|m| m.vault_db_sha256.clone()),
            local_last_synced_sha256,
            remote_ahead,
            local_dirty,
            in_sync,
            remote_check_skipped_offline: false,
            last_synced_at: state.last_synced_at,
            remote_sequence: remote_manifest.as_ref().map(|m| m.sequence).unwrap_or(0),
        })
    }

    /// Check Google Drive sync status
    pub fn status_gdrive(storage: &Storage) -> Result<SyncStatusReport> {
        let cfg = LightMemConfig::load();
        Self::status_gdrive_with_config(storage, &cfg)
    }

    pub fn status_gdrive_with_config(
        storage: &Storage,
        config: &LightMemConfig,
    ) -> Result<SyncStatusReport> {
        if config.is_offline() {
            return Self::status(storage, config, None);
        }

        let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
            anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
        })?;
        let folders = client.ensure_folders()?;
        let target_display = PathBuf::from(format!("gdrive://LightMem ({})", client.user_email));

        let blobs = client
            .list_existing_blobs(&folders.blobs_id)
            .unwrap_or_default();
        let snapshot_files = client
            .list_existing_blobs_with_ids(&folders.snapshots_id)
            .unwrap_or_default();
        let remote_snapshot_exists = snapshot_files.contains_key(SYNC_ZIP_FILENAME)
            || snapshot_files.contains_key(SYNC_LEGACY_DB_FILENAME);
        let remote_manifest = client
            .download_optional_file_by_name("manifest.json", &folders.root_id)?
            .and_then(|b| Self::parse_manifest_bytes(&b));

        let all = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;
        let local_only = all.iter().filter(|m| m.is_local()).count();
        let syncable = all.len().saturating_sub(local_only);
        let daemon_pid = Self::get_daemon_status();
        let daemon_running = daemon_pid.is_some();

        let state = SyncState::load_for_db(storage.path());
        let (remote_ahead, local_dirty, in_sync, local_last_synced_sha256) =
            Self::compute_sync_flags(
                storage,
                "gdrive",
                remote_manifest.as_ref(),
                remote_snapshot_exists,
                syncable,
            )?;

        Ok(SyncStatusReport {
            target: target_display,
            configured: true,
            target_exists: true,
            blobs_count: blobs.len(),
            total_blob_bytes: 0,
            snapshots_count: snapshot_files.len(),
            latest_snapshot_date: remote_manifest.as_ref().map(|m| m.last_synced_at),
            local_only_memories_count: local_only,
            syncable_memories_count: syncable,
            daemon_pid,
            daemon_running,
            offline: false,
            remote_manifest_exists: remote_manifest.is_some(),
            remote_sha256: remote_manifest.as_ref().map(|m| m.vault_db_sha256.clone()),
            local_last_synced_sha256,
            remote_ahead,
            local_dirty,
            in_sync,
            remote_check_skipped_offline: false,
            last_synced_at: state.last_synced_at,
            remote_sequence: remote_manifest.as_ref().map(|m| m.sequence).unwrap_or(0),
        })
    }

    /// Pull & merge (when `restore_db == false`, skipping download if already in sync) or full DB restore (when `restore_db == true`)
    pub fn pull(
        storage: &Storage,
        config: &LightMemConfig,
        custom_target: Option<&Path>,
        restore_db: bool,
    ) -> Result<SyncPullReport> {
        if config.is_offline() {
            anyhow::bail!(
                "LightMem sync is currently OFFLINE. Run 'lmem sync --online' (or unset LIGHTMEM_OFFLINE) to resume cloud sync."
            );
        }

        let is_gdrive = Self::resolve_target_is_gdrive(config, custom_target);
        if is_gdrive {
            if !GoogleDriveClient::is_connected() {
                anyhow::bail!(
                    "Google Drive is not connected.\nRun 'lmem auth login' to authenticate, or provide an explicit local backup path via '--target <PATH>'."
                );
            }
            let _lock = SyncLock::acquire(storage.path())?;
            let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
                anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
            })?;
            let folders = client.ensure_folders()?;
            return Self::pull_gdrive_locked(storage, &client, &folders, restore_db);
        }

        let target = custom_target
            .map(Path::to_path_buf)
            .or_else(|| config.effective_sync_target())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No sync target configured. Specify via --target <PATH> or 'lmem config --sync-target <PATH>'"
                )
            })?;

        anyhow::ensure!(
            target.exists(),
            "Sync target directory '{}' does not exist",
            target.display()
        );

        let _lock = SyncLock::acquire(storage.path())?;
        Self::pull_local_locked(storage, &target, restore_db)
    }

    fn pull_local_locked(
        storage: &Storage,
        target: &Path,
        restore_db: bool,
    ) -> Result<SyncPullReport> {
        let blobs_dir = target.join("blobs");
        let snapshots_dir = target.join("snapshots");

        let blobs_found = if blobs_dir.exists() {
            fs::read_dir(&blobs_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file() && !e.path().to_string_lossy().contains(".tmp."))
                .count()
        } else {
            0
        };

        let target_key = local_target_key(target);
        let remote_manifest = Self::read_local_manifest(target);
        let zip_snap = snapshots_dir.join(SYNC_ZIP_FILENAME);
        let legacy_snap = snapshots_dir.join(SYNC_LEGACY_DB_FILENAME);
        let preferred_snap = match remote_manifest
            .as_ref()
            .and_then(|m| m.snapshot_filename.as_deref())
        {
            Some(name) if name == SYNC_LEGACY_DB_FILENAME && legacy_snap.exists() => {
                Some((legacy_snap.clone(), false))
            }
            _ if zip_snap.exists() => Some((zip_snap.clone(), true)),
            _ if legacy_snap.exists() => Some((legacy_snap.clone(), false)),
            _ => None,
        };

        let syncable_now = storage
            .list_memories(None, Some(MemoryStatus::Active), None, 0)?
            .into_iter()
            .filter(|m| !m.is_local())
            .count();
        let (remote_ahead, was_local_dirty, _, _) = Self::compute_sync_flags(
            storage,
            &target_key,
            remote_manifest.as_ref(),
            preferred_snap.is_some(),
            syncable_now,
        )?;

        if !restore_db && (preferred_snap.is_none() || !remote_ahead) {
            return Ok(SyncPullReport {
                target: target.to_path_buf(),
                blobs_found_in_target: blobs_found,
                restored_db: false,
                restored_snapshot_path: None,
                skipped_up_to_date: true,
                merged_memories: 0,
                updated_memories: 0,
                deleted_memories: 0,
                local_kept_newer: 0,
                remote_portable_config: remote_manifest
                    .as_ref()
                    .and_then(|m| m.portable_config.clone()),
            });
        }

        let (snap_path, is_zip) = preferred_snap.ok_or_else(|| {
            anyhow::anyhow!(
                "No {} or {} found in '{}' to pull from",
                SYNC_ZIP_FILENAME,
                SYNC_LEGACY_DB_FILENAME,
                snapshots_dir.display()
            )
        })?;

        let tmp_dir = tempfile::tempdir()?;
        let staged_db_path = tmp_dir.path().join("staged_snapshot.db");
        if is_zip {
            extract_db_from_zip_streaming(&snap_path, &staged_db_path)?;
        } else {
            fs::copy(&snap_path, &staged_db_path).with_context(|| {
                format!(
                    "Failed to stage snapshot {} for CAS rehydration",
                    snap_path.display()
                )
            })?;
        }

        let actual_sha = compute_file_sha256_hex(&staged_db_path)?;
        if let Some(ref m) = remote_manifest {
            if !m.vault_db_sha256.is_empty() {
                anyhow::ensure!(
                    actual_sha == m.vault_db_sha256,
                    "Snapshot integrity check failed (expected sha256:{}, got sha256:{})",
                    m.vault_db_sha256,
                    actual_sha
                );
            }
        }

        let staged = Storage::open(&staged_db_path)?;
        if Storage::file_has_dehydrated_attachments(&staged_db_path)? {
            Self::rehydrate_local_staged_db_with_vault(&staged, Some(storage), &blobs_dir)?;
        }

        let mut merge_stats = crate::repositories::SyncMergeStats::default();
        if restore_db {
            staged.flush_to_single_file()?;
            drop(staged);
            storage.restore_from_db_file(&staged_db_path)?;
        } else {
            merge_stats = storage.merge_sync_vault(&staged)?;
            drop(staged);
        }

        let mut state = SyncState::load_for_db(storage.path());
        state.last_synced_target = Some(target_key);
        state.last_synced_sha256 = Some(
            remote_manifest
                .as_ref()
                .map(|m| m.vault_db_sha256.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or(actual_sha),
        );
        if restore_db || !was_local_dirty {
            state.last_synced_revision = storage.storage_revision()?;
        }
        if let Some(ref m) = remote_manifest {
            state.last_synced_sequence = state.last_synced_sequence.max(m.sequence);
            state.last_synced_at = Some(m.last_synced_at);
        } else {
            state.last_synced_at = Some(Utc::now());
        }
        let _ = state.save_for_db(storage.path());

        Ok(SyncPullReport {
            target: target.to_path_buf(),
            blobs_found_in_target: blobs_found,
            restored_db: restore_db,
            restored_snapshot_path: Some(snap_path),
            skipped_up_to_date: false,
            merged_memories: merge_stats.inserted_memories,
            updated_memories: merge_stats.updated_memories,
            deleted_memories: merge_stats.deleted_memories,
            local_kept_newer: merge_stats.local_kept_newer,
            remote_portable_config: remote_manifest.and_then(|m| m.portable_config),
        })
    }

    fn find_local_blob_path(blobs_dir: &Path, sha256: &str, filename: &str) -> Option<PathBuf> {
        let exact = blobs_dir.join(cas_blob_filename(sha256, filename));
        if exact.is_file() {
            return Some(exact);
        }
        let prefix = format!("{}.", sha256);
        let entries = fs::read_dir(blobs_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(&prefix) && !name.contains(".tmp.") {
                    return Some(path);
                }
            }
        }
        None
    }

    fn rehydrate_local_staged_db_with_vault(
        staged: &Storage,
        existing_local_vault: Option<&Storage>,
        blobs_dir: &Path,
    ) -> Result<()> {
        let dehydrated = staged.list_dehydrated_attachments()?;
        if dehydrated.is_empty() {
            return Ok(());
        }

        let mut grouped: BTreeMap<String, Vec<AttachmentMetadata>> = BTreeMap::new();
        for att in dehydrated {
            let key = cas_blob_filename(&att.sha256, &att.filename);
            grouped.entry(key).or_default().push(att);
        }

        for (blob_name, atts) in grouped {
            if let Some(vault) = existing_local_vault {
                if let Some(local_bytes) =
                    vault.get_populated_attachment_bytes(&atts[0].sha256, atts[0].size_bytes)?
                {
                    for att in &atts {
                        staged.rehydrate_attachment(
                            &att.id,
                            &att.sha256,
                            att.size_bytes,
                            &local_bytes,
                        )?;
                    }
                    continue;
                }
            }

            let blob_path =
                Self::find_local_blob_path(blobs_dir, &atts[0].sha256, &atts[0].filename)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Missing CAS blob '{}' in '{}' to rehydrate attachment '{}'",
                            blob_name,
                            blobs_dir.display(),
                            atts[0].filename
                        )
                    })?;
            let data = fs::read(&blob_path)
                .with_context(|| format!("Failed to read CAS blob from {}", blob_path.display()))?;
            for att in &atts {
                staged.rehydrate_attachment(&att.id, &att.sha256, att.size_bytes, &data)?;
            }
        }

        Ok(())
    }

    /// Pull & merge (or full restore when `restore_db == true`) from Google Drive
    pub fn pull_gdrive(storage: &Storage, restore_db: bool) -> Result<SyncPullReport> {
        let cfg = LightMemConfig::load();
        if cfg.is_offline() {
            anyhow::bail!(
                "LightMem sync is currently OFFLINE. Run 'lmem sync --online' (or unset LIGHTMEM_OFFLINE) to resume cloud sync."
            );
        }
        let _lock = SyncLock::acquire(storage.path())?;
        let client = GoogleDriveClient::try_load()?.ok_or_else(|| {
            anyhow::anyhow!("Google Drive not connected. Run 'lmem auth login' first.")
        })?;
        let folders = client.ensure_folders()?;
        Self::pull_gdrive_locked(storage, &client, &folders, restore_db)
    }

    fn pull_gdrive_locked(
        storage: &Storage,
        client: &GoogleDriveClient,
        folders: &crate::services::DriveFolders,
        restore_db: bool,
    ) -> Result<SyncPullReport> {
        let target_display = PathBuf::from(format!("gdrive://LightMem ({})", client.user_email));

        let remote_manifest = client
            .download_optional_file_by_name("manifest.json", &folders.root_id)?
            .and_then(|bytes| Self::parse_manifest_bytes(&bytes));

        let syncable_now = storage
            .list_memories(None, Some(MemoryStatus::Active), None, 0)?
            .into_iter()
            .filter(|m| !m.is_local())
            .count();

        let (remote_ahead, was_local_dirty, _, _) = Self::compute_sync_flags(
            storage,
            "gdrive",
            remote_manifest.as_ref(),
            remote_manifest.is_some(),
            syncable_now,
        )?;

        if !restore_db {
            if let Some(ref m) = remote_manifest {
                if !remote_ahead {
                    return Ok(SyncPullReport {
                        target: target_display,
                        blobs_found_in_target: m.total_blobs,
                        restored_db: false,
                        restored_snapshot_path: None,
                        skipped_up_to_date: true,
                        merged_memories: 0,
                        updated_memories: 0,
                        deleted_memories: 0,
                        local_kept_newer: 0,
                        remote_portable_config: m.portable_config.clone(),
                    });
                }
            }
        }

        // Prefer compressed vault_latest.db.zip; fall back to legacy vault_latest.db
        let (file_id, snap_name, is_zip) =
            if let Some(zip_id) = client.find_file(SYNC_ZIP_FILENAME, &folders.snapshots_id)? {
                (zip_id, SYNC_ZIP_FILENAME, true)
            } else if let Some(legacy_id) =
                client.find_file(SYNC_LEGACY_DB_FILENAME, &folders.snapshots_id)?
            {
                (legacy_id, SYNC_LEGACY_DB_FILENAME, false)
            } else if !restore_db {
                return Ok(SyncPullReport {
                    target: target_display,
                    blobs_found_in_target: 0,
                    restored_db: false,
                    restored_snapshot_path: None,
                    skipped_up_to_date: true,
                    merged_memories: 0,
                    updated_memories: 0,
                    deleted_memories: 0,
                    local_kept_newer: 0,
                    remote_portable_config: remote_manifest.and_then(|m| m.portable_config),
                });
            } else {
                anyhow::bail!(
                    "No {} or {} found in Google Drive snapshots folder",
                    SYNC_ZIP_FILENAME,
                    SYNC_LEGACY_DB_FILENAME
                );
            };

        let tmp_dir = tempfile::tempdir()?;
        let staged_db_path = tmp_dir.path().join("staged_gdrive.db");
        if is_zip {
            let tmp_zip = tmp_dir.path().join("downloaded.db.zip");
            client.download_file_to_path(&file_id, &tmp_zip)?;
            extract_db_from_zip_streaming(&tmp_zip, &staged_db_path)?;
        } else {
            client.download_file_to_path(&file_id, &staged_db_path)?;
        }

        let actual_sha = compute_file_sha256_hex(&staged_db_path)?;
        if let Some(ref m) = remote_manifest {
            if !m.vault_db_sha256.is_empty() {
                anyhow::ensure!(
                    actual_sha == m.vault_db_sha256,
                    "Google Drive snapshot integrity check failed (expected sha256:{}, got sha256:{})",
                    m.vault_db_sha256,
                    actual_sha
                );
            }
        }

        let mut blobs_found_in_target =
            remote_manifest.as_ref().map(|m| m.total_blobs).unwrap_or(0);
        let staged = Storage::open(&staged_db_path)?;
        if Storage::file_has_dehydrated_attachments(&staged_db_path)? {
            let dehydrated = staged.list_dehydrated_attachments()?;
            if !dehydrated.is_empty() {
                let mut grouped: BTreeMap<String, Vec<AttachmentMetadata>> = BTreeMap::new();
                for att in dehydrated {
                    let key = cas_blob_filename(&att.sha256, &att.filename);
                    grouped.entry(key).or_default().push(att);
                }

                let mut remaining_groups = Vec::new();
                for (blob_name, atts) in grouped {
                    if let Some(local_bytes) = storage
                        .get_populated_attachment_bytes(&atts[0].sha256, atts[0].size_bytes)?
                    {
                        for att in &atts {
                            staged.rehydrate_attachment(
                                &att.id,
                                &att.sha256,
                                att.size_bytes,
                                &local_bytes,
                            )?;
                        }
                    } else {
                        remaining_groups.push((blob_name, atts));
                    }
                }

                if !remaining_groups.is_empty() {
                    let blob_ids = client.list_existing_blobs_with_ids(&folders.blobs_id)?;
                    blobs_found_in_target = blob_ids.len();
                    for (blob_name, atts) in remaining_groups {
                        let sha_prefix = format!("{}.", atts[0].sha256);
                        let blob_file_id = blob_ids
                            .get(&blob_name)
                            .or_else(|| {
                                blob_ids
                                    .iter()
                                    .find(|(k, _)| k.starts_with(&sha_prefix))
                                    .map(|(_, v)| v)
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Missing CAS blob '{}' in Google Drive blobs folder to rehydrate attachment '{}'",
                                    blob_name,
                                    atts[0].filename
                                )
                            })?;
                        let blob_bytes = client.download_file(blob_file_id)?;
                        for att in &atts {
                            staged.rehydrate_attachment(
                                &att.id,
                                &att.sha256,
                                att.size_bytes,
                                &blob_bytes,
                            )?;
                        }
                    }
                }
            }
        }

        let mut merge_stats = crate::repositories::SyncMergeStats::default();
        if restore_db {
            staged.flush_to_single_file()?;
            drop(staged);
            storage.restore_from_db_file(&staged_db_path)?;
        } else {
            merge_stats = storage.merge_sync_vault(&staged)?;
            drop(staged);
        }

        let mut state = SyncState::load_for_db(storage.path());
        state.last_synced_target = Some("gdrive".to_string());
        state.last_synced_sha256 = Some(
            remote_manifest
                .as_ref()
                .map(|m| m.vault_db_sha256.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or(actual_sha),
        );
        if restore_db || !was_local_dirty {
            state.last_synced_revision = storage.storage_revision()?;
        }
        if let Some(ref m) = remote_manifest {
            state.last_synced_sequence = state.last_synced_sequence.max(m.sequence);
            state.last_synced_at = Some(m.last_synced_at);
        } else {
            state.last_synced_at = Some(Utc::now());
        }
        let _ = state.save_for_db(storage.path());

        Ok(SyncPullReport {
            target: target_display,
            blobs_found_in_target,
            restored_db: restore_db,
            restored_snapshot_path: Some(PathBuf::from(format!(
                "gdrive://snapshots/{}",
                snap_name
            ))),
            skipped_up_to_date: false,
            merged_memories: merge_stats.inserted_memories,
            updated_memories: merge_stats.updated_memories,
            deleted_memories: merge_stats.deleted_memories,
            local_kept_newer: merge_stats.local_kept_newer,
            remote_portable_config: remote_manifest.and_then(|m| m.portable_config),
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
            let _ = pid;
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

    /// Background daemon worker loop that reloads config live each cycle, pulls remote changes if ahead,
    /// pushes local changes when dirty, and respects `--offline` pause mode cleanly.
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
            let cycle_config = LightMemConfig::try_load().unwrap_or_else(|_| config.clone());
            if cycle_config.is_offline() {
                eprintln!(
                    "[{}] Sync daemon paused (--offline mode active). Skipping cycle.",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                );
                std::thread::sleep(interval);
                continue;
            }

            let now = Utc::now();
            eprintln!(
                "[{}] Sync daemon cycle starting...",
                now.format("%Y-%m-%d %H:%M:%S UTC")
            );
            match Self::status(storage, &cycle_config, custom_target) {
                Ok(st) => {
                    if st.remote_ahead {
                        match Self::pull(storage, &cycle_config, custom_target, false) {
                            Ok(rep) => {
                                eprintln!(
                                    "[{}] Daemon pulled remote changes: +{} new, ~{} updated, -{} deleted.",
                                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                    rep.merged_memories,
                                    rep.updated_memories,
                                    rep.deleted_memories
                                );
                            }
                            Err(e) => {
                                eprintln!(
                                    "[{}] Daemon pull error: {:#}",
                                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                    e
                                );
                            }
                        }
                    }
                    let need_push = st.local_dirty || !st.remote_manifest_exists;
                    if need_push {
                        match Self::push_with_options(
                            storage,
                            &cycle_config,
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
                    } else {
                        eprintln!(
                            "[{}] Vault already in sync (seq {}).",
                            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            st.remote_sequence
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[{}] Sync status check error: {:#}",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        e
                    );
                }
            }
            std::thread::sleep(interval);
        }
    }
}

fn target_key_exists(target_key: &str) -> bool {
    Path::new(target_key).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryRecord, MemoryType};

    #[test]
    fn test_streamed_zip_snapshot_roundtrip_and_invalid_zip_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let src_db = dir.path().join("src.db");
        let storage = Storage::open(&src_db).unwrap();
        let rec = MemoryRecord::new(
            MemoryType::Fact,
            "Zip Test".to_string(),
            "Verify streamed deflate roundtrip preserves SQLite bytes".to_string(),
            vec!["zip".to_string()],
            0.9,
            None,
        );
        storage.insert_memory(&rec, None).unwrap();
        storage.flush_to_single_file().unwrap();
        drop(storage);

        let expected_sha = compute_file_sha256_hex(&src_db).unwrap();
        let zip_path = dir.path().join("vault_latest.db.zip");
        let compressed_bytes = compress_db_to_zip_streaming(&src_db, &zip_path).unwrap();
        assert!(compressed_bytes > 0);

        let extracted_db = dir.path().join("extracted.db");
        extract_db_from_zip_streaming(&zip_path, &extracted_db).unwrap();
        let actual_sha = compute_file_sha256_hex(&extracted_db).unwrap();
        assert_eq!(actual_sha, expected_sha);

        // Corrupt zip fails cleanly
        let bad_zip = dir.path().join("corrupt.db.zip");
        fs::write(&bad_zip, b"not a zip file").unwrap();
        assert!(extract_db_from_zip_streaming(&bad_zip, &extracted_db).is_err());
    }

    #[test]
    fn test_sync_lock_stale_recovery_and_offline_guards() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("vault.db");
        let storage = Storage::open(&db_path).unwrap();

        // Simulate stale lock from an exited PID + old timestamp
        let lock_path = PathBuf::from(format!("{}.sync.lock", db_path.display()));
        fs::write(&lock_path, "999999\n2020-01-01T00:00:00Z\n").unwrap();
        {
            let _guard = SyncLock::acquire(&db_path).unwrap();
            assert!(lock_path.exists());
        }
        assert!(!lock_path.exists(), "Drop must release advisory lock");

        // Offline config blocks push/pull and allows status without hitting target
        let mut offline_cfg = LightMemConfig {
            backend: "hash".to_string(),
            sync_target: Some(dir.path().join("SyncTarget").to_string_lossy().to_string()),
            sync_offline: Some(true),
            ..Default::default()
        };
        let push_err = SyncService::push(&storage, &offline_cfg, None, false).unwrap_err();
        assert!(push_err.to_string().contains("OFFLINE"));
        let pull_err = SyncService::pull(&storage, &offline_cfg, None, false).unwrap_err();
        assert!(pull_err.to_string().contains("OFFLINE"));

        let st = SyncService::status(&storage, &offline_cfg, None).unwrap();
        assert!(st.offline);
        assert!(st.remote_check_skipped_offline);

        // Online toggle restores sync capability
        offline_cfg.sync_offline = Some(false);
        let online_push = SyncService::push(&storage, &offline_cfg, None, false).unwrap();
        assert!(online_push.snapshot_bytes > 0);
        let st_online = SyncService::status(&storage, &offline_cfg, None).unwrap();
        assert!(!st_online.offline);
        assert!(st_online.in_sync);
    }
}
