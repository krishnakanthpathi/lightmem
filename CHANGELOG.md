# Changelog

## 0.7.6

- Update GitHub Actions macOS build runners from retired `macos-14` to `macos-15` in `release.yml` and `pypi.yml` to unblock `aarch64-apple-darwin` binaries and `macosx_11_0_arm64` PyPI wheels.
- Prioritize `bonsai-2:2b` (`bonsai-2:latest`) for local Ollama auto-resolution (`ollama:auto`) and shell completions, skipping `-cloud` models on localhost endpoints while preserving `ollama-cloud` preset behavior.
- Harden `OllamaReranker` and `ObserverService` for reasoning models by sending `reasoning_effort: "none"` / `think: false` (with HTTP 400 fallback) and hardening balanced JSON extraction against `<think>...</think>` and duplicate pre-`</think>` payloads.
- Expand `OllamaReranker::answer` to inspect the top 10 gated candidates, add grounded multi-memory synthesis for summary/listing queries with strict qualifier checks, and boost digit-bearing ordinal/identifier query tokens during hybrid search.
- Guard `entity_index` QA shortcuts with whole-token matching, tautology/subject-in-question filtering, and explicit `--reranker` override adherence; honor local/offline reranker configs in `ObserverService::extract`.

## 0.7.5

- Fix TCP connection resets (`ECONNRESET`) in `lmem docs` / `doc` / `web` under browsers and large HTTP headers by draining incoming request headers cleanly before socket write shutdown, handling `HEAD` and `405`, and serving requests concurrently.
- Surface `stderr` warnings instead of silently ignoring errors when `--open` fails to invoke the OS default application (`open` / `xdg-open` / `cmd /C start`).
- Add permanent CLI E2E regression tests covering `docs` HTTP header draining, `--open` OS launcher integration, extension auto-tags, SHA-256 duplicate warnings, and multimodal `recall --image` over `/api/embed`.

## 0.7.4

- Add `--open` flag on `lmem recall` and `lmem inspect` to materialize and open verified attachments directly in the system default application (`open` on macOS, `xdg-open` on Linux, `cmd /C start` on Windows).
- Restore native multimodal vector embeddings (`embeddinggemma-2` via Ollama `/api/embed` for images, video keyframes via `ffmpeg`, and audio clips >= 0.5s) and visual image similarity search via `lmem recall --image <path>` (`--img`).
- Restore granular lowercase file-extension auto-tags (e.g., `#png`, `#mp4`) alongside media kind tags, plus batch and vault-wide SHA-256 duplicate attachment warnings (`find_attachments_by_sha256`).
- Restore unified vault telemetry dashboard via `lmem status` (`lmem vaults`, `lmem identity status`) and Python SDK `vault_status()`.
- Restore embedded offline documentation HTTP server via `lmem docs` (`lmem doc`, `lmem web`, `--port`, `--landing`, `--no-browser`) without any cloud sync dependencies.

## 0.7.3

- Restore the clean pure-local architecture (`v0.5.3` baseline) for consistent single-machine usage across macOS and Linux.
- Remove Google Drive login/OAuth, cloud sync, sync daemon, and embedded docs server subsystems.
- Add lossless Schema v7-to-v6 downgrade compatibility in SQLite vault migration so existing vaults from `v0.7.2` open and write cleanly without data loss.

## 0.5.3

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
