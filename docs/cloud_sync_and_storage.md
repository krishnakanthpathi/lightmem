# ❖ Cloud Sync, Storage & Documentation Guide (`lmem sync` & `lmem docs`)

Command-driven reference for Google Drive synchronization, Content-Addressed Storage (CAS), atomic vector snapshots, background daemons, and embedded documentation.

---

## 1. Quick Overview

LightMem provides decoupled cloud and network storage synchronization:
1. **Content-Addressed Storage (CAS)**: Media attachments (images, 4K videos, voice notes, PDFs) are stored by SHA-256 hash in `blobs/` on the remote target. Identical assets are deduplicated and uploaded only once.
2. **Atomic SQLite Vector Snapshots**: Flushed SQLite database checkpoints are backed up into `snapshots/vault_<timestamp>.db` on the remote target.
3. **Native Google Drive REST Integration**: Direct OAuth2 loopback authentication without requiring local Google Drive sync software or third-party background daemons.
4. **Air-Gapped Privacy (`--local`)**: Memories saved with `--local` are automatically tagged `#local` and completely exempt from cloud backups and remote snapshots.
5. **On-Demand Background Sync Daemon (`-d, --daemon`)**: Run continuous background sync with PID tracking, log inspection, and graceful termination.
6. **Embedded Documentation Server (`lmem docs`)**: Zero-dependency local documentation and landing page HTTP server bundled directly in the compiled binary.

---

## 2. Google Drive Authentication (`lmem auth`)

### Login via OAuth2 Loopback
```bash
lmem auth login
# Alias:
lmem login
```
* Opens the system browser to Google's OAuth consent screen.
* Spawns a temporary local loopback server to safely receive authorization code tokens.
* Securely stores tokens in `~/.lightmem/gdrive_token.json` (`0600` permissions).

### Check Authentication Status
```bash
lmem auth status
```
* Displays authenticated Google account email, token expiry, and storage quota.
* Example:
  ```text
  ◈ Google Drive OAuth Status
    Authenticated: Yes
    Account: krishnakanthpathi@gmail.com
    Scope: https://www.googleapis.com/auth/drive.file
    Target Folder: LightMem (ID: 1xABC...)
  ```

### Logout & Token Revocation
```bash
lmem auth logout
# Alias:
lmem logout
```
* Sends token revocation request directly to Google authorization servers.
* Deletes local cached tokens (`gdrive_token.json`) and folder cache (`gdrive_folder_id.txt`).

---

## 3. Synchronizing Vaults (`lmem sync`)

### Push Sync (Default)
```bash
lmem sync
```
* If authenticated with Google Drive, syncs directly to `gdrive://LightMem`.
* Scans local active memories, verifies missing CAS blobs, and uploads new media attachments.
* Renders real-time byte transfer progress meters: `[▓▓▓▓░░░░] 24.5/50.0 MB (49%)`.
* Generates an atomic vector snapshot of the SQLite database and uploads it to the remote target.
* Updates `manifest.json` on the remote target.

### Selective Component Syncing
```bash
# Only backup SQLite database snapshots, skipping attachment uploads
lmem sync --backup-only
# Alias:
lmem sync --skip-media

# Only sync CAS media attachments, skipping SQLite database snapshot
lmem sync --skip-db

# Skip interactive confirmation prompts
lmem sync -y
# Alias:
lmem sync --yes
```

### Inspect Sync Status
```bash
lmem sync --status
```
* Shows remote target existence, total uploaded CAS blobs, snapshot count, syncable memory count, and background daemon status.

### Pull & Restore Remote Vaults
```bash
# Pull missing media attachments from sync target
lmem sync --pull

# Restore local vault from latest remote database snapshot
lmem sync --restore
```

### Custom Sync Targets
```bash
# Sync to custom local network share or external drive
lmem sync --target /Volumes/SecureStorage/LightMemVault
```

---

## 4. Background Sync Daemon (`-d, --daemon`)

For autonomous agents and continuous workflows, `lmem sync` can run as a detached background daemon:

### Start Daemon
```bash
lmem sync -d
# With options:
lmem sync -d --skip-db
```
* Spawns a detached background process that periodically syncs changes every 5 minutes.
* Writes process ID to `~/.lightmem/sync_daemon.pid`.
* Writes continuous cycle output to `~/.lightmem/sync_daemon.log`.

### Check Daemon Status
```bash
lmem sync --status
```
* Displays `Sync Daemon: Running (PID: 12345) · Log: ~/.lightmem/sync_daemon.log` or `Sync Daemon: Inactive`.

### Stop Daemon
```bash
lmem sync --stop
```
* Sends termination signal to the running daemon process and cleans up PID tracking.

---

## 5. Air-Gapped Local Memories (`--local`)

Keep sensitive notes, local credentials, or private scratchpads strictly offline:

```bash
lmem remember "AWS Root Keys in 1Password Vault" --local
```
* Adds the `#local` tag automatically.
* Completely excluded from `lmem sync` CAS uploads and remote database snapshots.
* Excluded from cloud manifest record counts.

---

## 6. Embedded Documentation Server (`lmem docs`)

Serve the complete LightMem documentation and landing page locally with zero external web servers or internet access:

```bash
# Launch docs server and open default browser
lmem docs
# Aliases:
lmem doc
lmem web

# Bind to a custom port
lmem docs --port 8088

# Run headlessly without launching system browser
lmem docs --no-browser
```

### Endpoints
* **Landing Page**: `http://localhost:8080/`
* **Technical Docs**: `http://localhost:8080/docs`
* **Styles & Scripts**: Bundled statically in binary (`/docs.css`, `/docs.js`, `/theme.css`, `/logo.png`).
* **Zero Daemons**: Runs purely in the foreground and terminates cleanly upon `Ctrl+C`.
