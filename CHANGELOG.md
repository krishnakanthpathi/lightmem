# Changelog

## 0.10.0

- **Modular CLI Architecture**: Modularized `main.rs` from 5,000+ monolithic lines into maintainable domain-specific command handlers (`src/commands/`), eliminating raw database queries in views and CLI helpers.
- **Global Flag Standardization & JSON Output**: Standardized `--json` as a true global flag across all subcommands, harmonized export formatting with `--format json|okf|bundle`, and added uniform `-y, --yes` confirmation gates for destructive actions (`forget --all --hard`, `clear`, `config --reset-db`, `reindex`).
- **SQLite Migrations & Concurrency Safety**: Guaranteed safe version schema upgrades without dropping tombstones or table data, computed vector embeddings before acquiring SQLite transactions to eliminate I/O lock contention, and added file permission hardening (`0o600` on WAL/SHM files).
- **FTS5 & Vector Scaling Optimizations**: Converted FTS5 triggers to external-content indexing with rowid synchronization, only firing index updates when indexed fields mutate; moved pagination and date filtering directly into SQL queries.
- **Search & Reranker Correctness**: Enforced similarity thresholds across all recall paths, assigned contiguous vector ranks post-filtering, isolated corrupt/mismatched vector records during multi-item recall, and enabled streaming downloads for local ONNX assets.
- **API Server & MCP Protocol Hardening**: Secured the HTTP API daemon to `127.0.0.1` by default with constant-time Bearer token comparisons; upgraded MCP implementation to full JSON-RPC 2.0 compliance (-32600 for invalid requests, -32602 for missing tools, reused persistent controller instances).

## 0.9.0

- **Streamlined Interactive Configuration (`lmem config --edit`)**: Numbered sub-menus (1..10 for answer generation, 1..6 for vector embeddings, 1..2 for extraction and remote routing) with direct recommended defaults on `<Enter>`, preventing infinite re-prompt loops and eliminating double-prompt friction.
- **Categorized 11-Profile Setup Experience (`lmem setup`)**: Pixel-perfect columnar interactive setup grouped into Local & Offline (ONNX, Ollama, Demo), Preconfigured Cloud LLMs (OpenAI, Gemini, Groq, OpenRouter, DeepSeek, Mistral), and Custom Endpoints / Remote Vaults.
- **First-Class One-Command Model Switching (`lmem model`)**: Switch answer models and providers with a single command (`lmem model <provider:model>`). Automatically aligns endpoints, offers interactive API key configuration when missing, and displays active provider status.
- **Decoupled Search Embeddings & Generation Endpoints**: Dedicated `--embedding-endpoint` and `--embedding-api-key` settings and CLI flags prevent cloud LLM endpoints from conflicting with Ollama or local vector search.
- **Per-Query Model Overrides (`-m, --model`)**: Ad-hoc model testing via `lmem answer "..." -m <provider:model>` with dedicated `--model-api-key` support and branded error cards on 401/403/connection failures.
- **Universal Installer Upgrades**: Standalone `install.sh` and `install.ps1` with animated step loaders, IPv6 dual-stack fallback, and automated shell PATH persistence.

## 0.8.4

- **Unified & Simplified Provider Presets**: Streamlined model and provider configuration with pre-built presets (`local`, `ollama`, `openai`, `gemini`, `groq`, `openrouter`, `deepseek`, `mistral`, `custom`, `demo`). Selecting a provider preset automatically configures official endpoint URLs, requiring only the user's API key without URL boilerplate.
- **Dedicated Vector Embedding Endpoints**: Completely separated vector search embedding configuration from answer generation endpoints. Added dedicated `--embedding-endpoint` and `--embedding-api-key` flags and configuration fields, preventing cloud LLM endpoints from conflicting with or leaking into Ollama or custom embedding backends.
- **One-Command Model Switching (`lmem model`)**: Added first-class `lmem model [provider_or_model] [--api-key KEY] [--endpoint URL]` command to quickly view the current setup or switch answer models and providers with a single command.
- **Clean Two-Card Configuration Display**: Rebuilt `lmem config` into two clear visual cards (`ANSWER GENERATION ENGINE` and `VECTOR SEARCH EMBEDDINGS`), eliminating ambiguous legacy configuration displays while maintaining full backwards compatibility for existing config files.
- **Explicit Custom Endpoints**: Custom proxy and self-hosted OpenAI-compatible services are configured explicitly via `--preset custom --endpoint <url>`, eliminating confusing automatic name-guessing heuristics.

## 0.8.3

- **Command History & Audit Trail**: Added `lmem history` (and alias `lmem hist`) command backed by an isolated local SQLite database (`~/.lightmem/history.db`, mode `0600`). Records executed command history, status (`✓`/`✗`), duration, relative timestamps, search query filtering (`lmem history <query>`), status filtering (`--status success|error`), limiting (`-n, --limit`), clearing (`--clear`), and JSON output (`--json`), with strict secret redaction (`--api-key`, `--openai-api-key`, etc.).
- **Branded Model Error Cards**: Generative LLM and extractive QA HTTP failures (401 Unauthorized, 403 Forbidden, 404 Not Found, 429 Rate Limited, timeouts, and network unreachable) now render structured, branded terminal error cards (`CliErrorCard`) with clear root causes and actionable troubleshooting steps instead of raw provider stack traces.
- **Dynamic Model Provider Resolution**: The `lmem` home page banner, `lmem config`, and `lmem answer` now dynamically inspect and display the active model provider and resolved model (e.g. `[reranker: groq:qwen/qwen3.8-27b]`, `Ollama Model: ollama (auto: bonsai-2:2b)`) instead of hardcoded `ollama:` prefixes or static strings.
- **Multi-Memory Bullet Answers & Grounding Fix**: `lmem answer` now instructs LLMs to return concise bullet points for multi-memory and multi-item answers, formats multiline bullets cleanly in the terminal, accepts grounded multi-index / natural-language answers without false `Insufficient evidence` rejections, and respects `--limit` beyond 10 candidates (up to 30).

## Unreleased — simpler terminal commands

- Add first-class `lmem local` (alias: `disconnect`) and `lmem remote` commands with interactive URL/key prompting, actionable offline hints, and updated Welcome Screen / Banner quick commands featuring `remote`, `local`, and `help`.
- Add numbered `config --edit` prompts, shared configuration previews, hidden credential input, and explicit embedding/answer-model flag aliases.
- Default guided local setup to ONNX search/answers with entity extraction off; offer model preparation separately and reuse CLI setup from installers.
- Add read-only `doctor` diagnostics and live connection-name completion for Bash, Zsh, and Fish.
- Keep script/JSON use noninteractive and preserve migration approval; shorten the welcome output to useful commands and the active vault.

## 0.8.0 — pending user verification

- Add terminal configuration previews (`config/setup --dry-run`), saved-embedding diagnostics (`config --test`), a no-download `demo` preset, explicit LLM key/entity controls, and readable local context. Setup reviews shared settings before saving; noninteractive setup requires explicit choices and confirmation. Guard `config --reset-db` with `--yes` and reject conflicting options before deletion.
- Add CLI `restore` for archived records, preserving IDs and attachments while clearing old expiry; reject ambiguous recovery titles. Configuration-only changes no longer create a local vault.
- Add `lmem ui`, an authenticated loopback workspace for memory search and sourced answers, text/file ingestion, archive/recovery, explicit vault/scope selection, named connections, configuration previews, model/reindex jobs, and bundle backup/restore review.
- Reject malformed/incomplete clear requests and blank API keys; redact both secrets in configuration output. Bound HTTP headers/concurrency, authenticate before body allocation, enforce browser origin/loopback-host rules, and omit private vault paths from public health responses.
- Reject unsupported remote commands/options instead of falling back locally. Preserve confidence, subsecond TTL, Unicode/reserved-character titles, temporal filters, media pagination, inspection links, and daemon scope/session. Align precision recall with the configured answer engine.
- Default LLM entity extraction to off, expose an explicit configuration setting, and enforce offline restrictions. Reject unavailable import enrichment instead of ignoring it; include both QA models in `config --download all`.
- Guard workspace writes against stale vault context and configuration changed by another process. Preview restore destinations and reject stale restore previews.
- Refresh documentation, Windows/server/workspace setup, runnable first-memory examples, accessible reading/control styles, command search, release metadata, and measured-performance wording.
- Generate release archive SHA-256 manifests, verify executable downloads, and validate Python archive extraction. Remove the unverified manylinux wheel claim, pin the Linux wheel build runner, and add PR/main validation.
- Add regression coverage for safety boundaries, remote correctness, configuration jobs, restore/recovery, scoped attribution, stale tabs, download integrity, and documentation contracts. Live models and native release platforms require separate verification before publication.

## 0.7.9

- **Persistent Backend Server**: Added `lmem serve` multi-threaded HTTP REST API server with Bearer token authentication, CORS support, LAN binding (`--host 0.0.0.0 --port <PORT>`), and seamless remote CLI routing (`lmem config --remote <URL> --api-key <KEY>`).
- **Daemon Background Task Management**: Added `--daemon` (`-d`), `--stop`, and `--status` flags to `lmem serve` to run the persistent server in the background as a detached daemon process, monitor its PID and health over HTTP, and terminate it cleanly.
- **Local Database Precedence**: Fixed CLI database routing so explicit local database flags (`--db <path>`, `LIGHTMEM_DB`) take precedence over configured remote URLs, preventing unwanted remote delegation during local operations.
- **Universal Installer Release Pipeline**: Formatted all Rust sources to strict `cargo fmt` specifications, ensuring GitHub Actions CI builds pre-compiled standalone release binaries across macOS, Linux, Windows, and Android Termux for instant single-line curl installation.

## 0.7.7

- Fix loopback HTTP mock servers across Ollama unit and integration tests (`reranker.rs`, `ollama_auth.rs`, `cli_migration.rs`, `func_integrity_services.rs`) to frame complete HTTP headers + `Content-Length` bodies before replying, eliminating the TCP split-read race that broke `v0.7.6` CI (`exit 101`) so macOS 15 ARM64 binaries and PyPI wheels publish cleanly.
- Harden knowledge-graph link visibility on soft forget (`status = 'expired'`): `get_links_for_memory` (`lmem inspect`), `traverse_multi_hop` (`lmem related`), and `get_graph_snapshot` (`lmem graph`) now filter out expired endpoints and prevent expired nodes from bridging multi-hop neighborhoods, while preserving lossless `all_links()` / `export --json` backups until `--hard` cascade deletion.
- Add regression test `test_forget_soft_and_hard_prunes_links_without_error` verifying error-free link pruning across both soft and hard forget paths.

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
