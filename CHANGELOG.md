# Changelog

## 0.7.0

- Fix multi-hundred-MB memory spikes and redundant media uploads during `lmem remember` inline auto-sync and `lmem sync`.
- Generate slim CAS-backed SQLite snapshots (`vault_latest.db`) during normal sync by dehydrating attachment BLOBs whose `<sha256>.<ext>` payload is already verified in `blobs/`, avoiding re-copying and re-uploading 100s of MBs of immutable media on every new text memory (`--backup-only` / `--skip-media` continues to produce self-contained full snapshots).
- Lazily check attachment existence by metadata (`sha256` and `size_bytes`) before reading any single missing BLOB payload from SQLite, freeing each uploaded payload buffer immediately after transfer.
- Replace full-file RAM buffering (`fs::read` + double multipart copies) with streaming 64 KB chunked SHA-256 hashing (`compute_file_sha256_hex`) and disk-streamed Google Drive uploads/updates/downloads (`upload_file_multipart_from_path_with_progress`, `upload_or_update_file_from_path_with_progress`, `download_file_to_path`).
- Automatically rehydrate CAS-dehydrated attachments from `blobs/` into a staged temporary vault with full byte-length and SHA-256 verification before atomically restoring on `lmem sync --restore`.

## 0.6.1

- Add unified visual memory vault & connection dashboard (`lmem status`, `lmem vaults`, and `lmem identity status`) displaying default vault path, active/total memory telemetry, and registered harness connections with physical database paths.
- Add `--json` output support to `lmem status` and `lmem identity status` for machine-readable vault topologies.
- Add `vault_status()` method to Python SDK `LightMem` client returning structured vault status overview.

## 0.6.0

- Add background sync daemon support via `lmem sync -d` / `--daemon`, with process tracking (`~/.lightmem/sync_daemon.pid`), status inspection (`lmem sync --status`), and graceful termination (`lmem sync --stop`).
- Add standalone zero-dependency documentation and landing page HTTP server embedded directly in binary (`lmem docs`, `lmem doc`, `lmem web`).
- Remove ambiguous silent fallback to local `~/Library/CloudStorage/GoogleDrive-*` paths, enforcing explicit Google Drive OAuth authentication.
- Add dynamic adaptive upload timeouts and real-time terminal transfer progress meters for large SQLite snapshots and 4K media attachments.
- Add `--skip-db`, `--skip-media` / `--backup-only`, and `-y`/`--yes` flags to `lmem sync`.
- Add direct Google Drive REST API cloud sync engine via native OAuth2 loopback authentication.
- Add `lmem auth` command suite (`status`, `login`, `logout`) with token revocation against Google authorization servers.
- Add `lmem logout` top-level command to invalidate OAuth tokens and delete local token/folder caches.
- Add `lmem login` for seamless browser OAuth popup with local loopback callback server.
- Support `LIGHTMEM_HOME` environment variable for configuration and token isolation in tests and scripts.

## 0.5.4

- Add native joint multimodal vector embedding support for Google `embeddinggemma-2` (768-dimensional unified space for text, code, images, video keyframes, and audio).
- Automatically extract and encode primary image/keyframe/audio payloads during memory indexing and re-indexing (`embed_multimodal` / `embed_multimodal_batch`).
- Add image-based query recall (`lmem recall --image <path>`) for direct cross-modal visual similarity retrieval.
- Add granular format extension tagging (`#png`, `#jpg`, `#webp`, `#wav`, `#mp4`) alongside broad `media_kind` tags upon attachment ingestion.
- Add SHA-256 Content-Addressed Storage (CAS) attachment deduplication with clear CLI warning notifications on existing vault assets (zero toast/popups).
- Add `--local` flag to `lmem remember` to strictly air-gap memories offline (`#local` tag, exempt from cloud sync and backup snapshots).
- Add decoupled CAS cloud sync & atomic SQLite vector snapshot backups (`lmem sync` with `--target`, `--status`, `--pull`, `--restore`, `--backup-only`).
- Support synchronous inline sync on memory creation when a sync target is configured (zero background daemon processes).
- Add `--open` flag to `lmem recall` and `lmem inspect` to open materialized attachment media directly in the operating system default application.
- Preserve backward-compatible fallback for text-only embedders (`OnnxEmbeddingProvider`, `HashEmbeddingProvider`).


- Fix architecture mismatch crashes when scanning or running native binaries (e.g., ARM binaries on AMD/x86_64 processors).
- Add strict ELF, Mach-O, and PE CPU architecture header validation in `_is_native_binary()` to prevent cross-architecture execution errors.
- Fix Termux `PermissionError: [Errno 13]` when probing restricted directories in `$PATH` (such as `/system/xbin/lmem`).
- Exclude native binary cache from wheel distribution package in `pyproject.toml` to prevent cross-architecture wheel pollution.
- Improve `execute_lmem()` failure diagnostics with clear architecture mismatch error guidance.

## 0.5.2

- Fix Linux `install.sh` and Windows `install.ps1` installer download targets and bump default release version to `v0.5.2`.
- Add installer option to skip local ONNX embedding & QA model downloads via `--skip-models` / `--no-models`, `LIGHTMEM_SKIP_MODELS=1`, and interactive menu choice `[5] none / skip`.
- Automatic LLM knowledge graph entity extraction during ingestion with `--no-entities` CLI flag and Python SDK support.
- Entity deduplication, conflict resolution, and persistence in SQLite vault.
- Web documentation and landing page updates.

## 0.5.0

- Add single-file compressed database transfer bundles (`.zip` / `.lmbundle`) packaging the clean SQLite database, all attached videos, images, audio, documents, vector embeddings, and knowledge graph links with SHA-256 integrity verification.
- Add `lmem bundle export [-o <path>]` to snapshot the database cleanly via `VACUUM INTO` and package all attached media into a universal `.zip` archive.
- Add `lmem bundle import <path>` to restore or merge bundles with automatic media extraction into the destination vault's `attachments/` directory without re-computing embeddings.
- Add `lmem bundle info <path>` to inspect transfer bundle manifests, memory counts, and video/photo media summaries without unpacking.
- Wire `--bundle` flag into `lmem export` and auto-detect `.zip` / `.lmbundle` archives in `lmem import`.
- Add Python SDK methods `bundle_export()`, `bundle_import()`, and `bundle_info()`.
- Add comprehensive integration test suite verifying byte-for-byte video and photo preservation across export, inspect, restore, and merge cycles.

## 0.4.0

- Register stable users, agents, built-in or custom harnesses, and named connections.
- Select connections with `--connection` / `LIGHTMEM_CONNECTION` and inspect them with `context`.
- Share project decisions across harnesses using the same user/project; keep user and agent scopes in separate SQLite vaults. Agent scopes also separate projects.
- Record the originating connection, user, agent, harness, project, scope, and optional session on memories; preserve this metadata through backups.
- Generate bound skill and MCP configurations. Custom harness registrations can specify a persistent skill installation path.
- Add Python SDK registration methods and `LightMem(connection=..., scope=..., session=...)`, `context()`, and MCP setup support. MCP writes accept a per-call session ID.
- Verify wheel RECORD hashes after native retagging and fail downloads instead of falling back to an incompatible older engine.
- Gate both release workflows on Rust and Python tests and smoke test installed wheels.
- Migrate schema v4 to v5 transactionally. Existing memories retain unknown source identity and are not automatically reassigned to a registered user.
- Reject ambiguous database/connection options and preserve invalid MCP configuration files instead of replacing them.

Each command operates on one selected scope; recall does not combine user, project, and agent vaults. Identity scopes organize local memory and do not provide authentication between operating-system users. Conversation extraction still requires `observe` or a harness invocation; installing a connection does not record chats automatically.
