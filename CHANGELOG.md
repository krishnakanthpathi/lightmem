# Changelog

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
