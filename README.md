# ❖ LightMem (`lmem`)

**Local-first persistent memory for AI agents: a Rust CLI, Python SDK, MCP server, and authenticated local workspace, backed by SQLite and configurable local or remote models.**

**0.8.0 review build:** changes are prepared locally and await verification before release. Start with the [CLI review guide](docs/cli-review-0.8.0.md); [optional browser/remote checks](docs/review-0.8.0.md) are also available.

[![GitHub Release](https://img.shields.io/github/v/release/krishnakanthpathi/lightmem?color=crimson&logo=github)](https://github.com/krishnakanthpathi/lightmem/releases)
[![PyPI version](https://img.shields.io/pypi/v/lmem.svg?color=blue&logo=pypi&logoColor=white)](https://pypi.org/project/lmem/)
[![Crates.io](https://img.shields.io/crates/v/lightmem.svg?color=orange&logo=rust)](https://crates.io/crates/lightmem)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/krishnakanthpathi/lightmem)
[![Local first](https://img.shields.io/badge/storage-local%20first-success.svg)](https://github.com/krishnakanthpathi/lightmem)

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **local ONNX Extractive QA (`minilm-squad2` / `tinyroberta-squad2`) & swappable Ollama rerankers** (`ollama:<model>`), **immutable SHA-256-verified multimodal BLOB attachments** (images, audio, video, documents), **Obsidian-grade `[[wikilinks]]` & multi-hop knowledge graph traversal**, **nearest-neighbor vector + overlap conflict merging**, **temporal point-in-time (`--as-of`) & single-day (`--date`) queries**, **14-category auto-inference**, and a **native Model Context Protocol (MCP) server** for AI agent IDEs.

---

## ◈ Table of Contents

- [Why LightMem?](#-why-lightmem)
- [Performance & Comparison](#-performance--comparison)
- [Quick Install](#-quick-install-macos-linux--windows)
- [Local management workspace](#local-management-workspace-080)
- [Python SDK Quickstart](#-python-sdk-quickstart)
- [Identity-Aware Connections](#identity-aware-connections-v040)
- [Single-File DB & Media Transfer Bundles](#single-file-db--media-transfer-bundles-v050)
- [Knowledge Graph Entities](#knowledge-graph-entities-v052)
- [Multimodal Embeddings & Direct Media Opening](#multimodal-embeddings--direct-media-opening-v074)
- [Embedded Local Documentation Server](#-embedded-local-documentation-server-lmem-docs)
- [Model Context Protocol (MCP) Server](#-model-context-protocol-mcp-server)
- [Core CLI Reference](#-core-cli-reference)
- [Switching Rerankers & Embedding Models](#-switching-rerankers--embedding-models)
- [Nearest-Neighbor Conflict Resolution](#-nearest-neighbor-conflict--duplicate-resolution-lmem-conflicts)
- [Temporal Filtering](#-temporal-filtering---as-of-vs---date)
- [Clean Uninstall](#-clean-uninstall)
- [Development & Testing](#-development--testing)
- [License](#-license)

---

## ⚡ Why LightMem?

- **Native CLI and lazy models**: SQL-only commands avoid loading embedding or answer models. Measure startup, recall, and QA separately on your target machine.
- **Choose the runtime**: Individual CLI/SDK commands run on demand. MCP, HTTP, and UI servers remain alive while used and can retain loaded models.
- **Local-first storage**: No required memory cloud. Cached ONNX inference runs locally. Ollama and LLM extraction can use remote endpoints; entity extraction is off by default. The local workspace has no analytics; the deployed website uses Vercel analytics.
- **Neural Extractive QA**: Extracts pinpoint answers (under 20 tokens) directly from stored memories instead of flooding LLM context windows.
- **Multimodal BLOB Vault**: Secure `0600` file storage for images, diagrams, audio, and 4K video with SHA-256 integrity checks and instant path materialization (`--path`).
- **Graph Multi-Hop Traversal**: Connect memories with Obsidian-style `[[wikilinks]]` or explicit directional edges and navigate relationship trees.
- **Review potential overlaps**: Vector similarity and word overlap identify candidate pairs. Local merging generally keeps the newer content; Ollama can synthesize a merge. Similarity alone does not prove a contradiction.
- **Portable DB & Media Transfer Bundles**: Export and import complete vaults—including SQLite tables, full vector embeddings, and all physical video, photo, audio, and PDF files—into a single compressed `.zip` bundle with zero re-indexing overhead.

---

## 📊 Performance and measurement

Latency depends on the corpus, model, hardware, cold/warm state, and network. SQL lookups, hybrid recall, and model-based answers are different operations; there is no universal sub-millisecond guarantee. The Python SDK starts a subprocess for each call. An exited CLI uses no idle process memory, while a running MCP/UI/HTTP server remains resident.

Run `cargo bench --bench bench_harness` for a host-specific report. It labels in-process timing, CLI measurements, nonempty-result rates, and integrity checks separately. Nonempty results do not establish retrieval accuracy. Publish the hardware, dataset size, model, runtime mode, and p50/p95 alongside any presentation numbers.

---

## ◈ Quick Install (macOS, Linux & Windows)

### Python (wheel with native executable)
```bash
pip install -U lmem
```

### Universal Shell Installer (macOS & Linux)
```bash
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh
# Optional: skip downloading local ONNX models during install (~130 MB saved)
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh -s -- --skip-models
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex
# Optional: skip downloading local ONNX models during install
$env:LIGHTMEM_SKIP_MODELS=1; irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex
```

### Build from Source (Rust / Cargo)
```bash
git clone https://github.com/krishnakanthpathi/lightmem.git
cd lightmem
cargo install --path . --force

# Or install directly into ~/.local/bin:
cargo install --path . --root ~/.local --force
```

---

## Local management workspace (0.8.0)

```bash
lmem ui                              # Open the authenticated loopback workspace
lmem --db ./demo.db ui                # Explicit local vault
lmem --connection codex --scope project ui --no-browser
```

Use the private startup URL to open the workspace. It binds only to `127.0.0.1`; its temporary token grants access to this server. The token is removed from the address bar after loading and retained in this tab’s session. Ctrl+C stops the server.

The workspace provides a visible vault/scope context, memory search and sourced answers, attachment downloads, add/archive/restore, named connections, model preparation, configuration preview and reindex approval, and bundle export/restore preview. Settings are shared under the configuration root; indexes are per vault. Remote CLI routing does not change this workspace’s local target. See the [review guide](docs/review-0.8.0.md) for a disposable demo.

---

## 🐍 Python SDK Quickstart

Install the official Python package with precompiled native wheels:

```bash
pip install -U lmem
```

Use `LightMem` directly in your AI agent pipelines with zero external dependencies:

```python
from lmem import LightMem

# Initialize vault (defaults to ~/.lightmem/memories.db)
lm = LightMem()

# 1. Store a memory (category auto-infers across 14 types if omitted)
memory = lm.remember(
    "PostgreSQL primary runs on port 5432 with WAL archiving enabled",
    category="fact",
    title="PostgreSQL Primary Config",
    tags=["postgres", "database", "production"],
    confidence=0.95,
)
print(f"Stored memory ID: {memory.id}")

# 2. Store Multimodal Attachments (images, audio, video, PDFs in 0600 BLOB vault)
from pathlib import Path
from tempfile import TemporaryDirectory
with TemporaryDirectory() as examples:
    notes = Path(examples) / "architecture-notes.txt"
    notes.write_text("API gateway connects to PostgreSQL", encoding="utf-8")
    diagram = lm.remember(
        "API Gateway routes to [[PostgreSQL Primary Config]]",
        files=[notes], tags=["architecture", "gateway"], extract=False,
    )

# 3. Hybrid Recall (FTS5 BM25 + Vector Cosine RRF + Graph Multi-Hop)
results = lm.recall("postgres port", limit=5, multi_hop=True)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# Materialize verified attachment file paths directly on disk
attachment_paths = lm.recall("gateway", media="document", limit=1, path=True)

# 4. Neural Extractive QA (extracts exact fact span via local ONNX SQuAD-2.0 or Ollama)
answer = lm.answer("what port does postgresql use?")
print(f"Extracted Answer: {answer.answer}")      # e.g. "5432"
print(f"Confidence: {answer.confidence:.2f}")

# 5. Conversational Observation & Candidate Extraction
candidates = lm.observe(
    "User: We switched Redis to port 6380.\nAssistant: Updated!",
    dry_run=True,
)
for c in candidates:
    print(f"Candidate: [{c.category}] {c.title}: {c.content}")

# 6. Intentional Knowledge Graph Linking & Multi-Hop Traversal
lm.link(diagram.id, memory.id, relation="depends_on", weight=1.0)
related = lm.related(memory.id, hops=2)

# 7. Conflict Detection & Telemetry
conflicts = lm.conflicts(min_similarity=0.80)
stats = lm.stats()
print(f"Vault: {stats.active_memories} memories, {stats.vector_count} indexed vectors.")

# 8. Single-File DB & Media Transfer Bundles (Zero Re-embedding)
# Packages checkpointed SQLite DB, all attached videos/photos/docs & vector embeddings into a .zip
lm.bundle_export("my-vault.zip")
info = lm.bundle_info("my-vault.zip")
print(f"Backed up {info['stats']['total_memories']} memories & {info['stats']['total_attachments']} attachments")
lm.bundle_import("my-vault.zip", mode="replace")  # or mode="merge"
```

---

## Identity-aware connections (v0.4.0)

Register the person, the logical agent, and a named harness connection once:

```bash
lmem identity user-add alice --name "Alice"
lmem identity agent-add coding --user alice
lmem identity connection-add agy --user alice --agent coding --harness antigravity --project myapp --memory-scope project
lmem identity connection-add codex --user alice --agent coding --harness codex --project myapp --memory-scope project

lmem --connection agy --session chat-123 remember "Decision: use SQLite WAL" -t decision
lmem --connection codex recall "database decision"
lmem --connection codex context --json
lmem status                          # Visual dashboard of all managed vaults, active tools, and storage
lmem vaults                          # Alias for lmem status (or lmem identity status)
lmem --connection codex --scope agent remember "Current debugging checkpoint"
lmem --connection agy connect
lmem --connection agy connect --mcp
```

Connections for the same user/project open the same persistent project vault across harnesses. `user` scope follows the user; `agent` scope belongs to the user/agent and, when provided, project. Each command uses one scope. Switch with `--scope`; recall does not blend scopes. `LIGHTMEM_CONNECTION` can select a default connection. Do not combine a connection with `--db`, `LIGHTMEM_DB`, or `--global`.

Register custom harnesses and deploy their bound instructions:

```bash
lmem identity harness-add my-harness --path /absolute/path/to/skills
lmem identity connection-add custom --user alice --agent coding --harness my-harness --memory-scope user
lmem --connection custom connect
lmem identity list connections --json
lmem identity list users --json
```

Custom MCP clients can launch `lmem --connection custom mcp`; automatic MCP configuration supports Antigravity, Claude, and Cursor. The MCP `context` tool shows the active binding. MCP `remember` also accepts a per-call `session_id` for conversation attribution. Connection IDs, user IDs, agent IDs, project IDs, and optional session IDs accept 1–128 ASCII letters, digits, underscores, or hyphens.

The registry is stored in `~/.lightmem/identities.db` and scoped vaults under `~/.lightmem/vaults/` (or `LIGHTMEM_CONFIG_DIR`). Scope keys use hashes to avoid filesystem case collisions. All memory operations, including graph traversal, deduplication, conflicts, attachments, and export, operate on the selected vault. New memories expose optional `context` source metadata in JSON; exact duplicates retain their original source. Backups preserve source metadata even when deliberately imported into another vault.

Existing commands and databases continue to work. Schema v5 adds optional source context; existing memories are not attributed to a user automatically. To copy existing memories into a new scope, explicitly export the old database and import into the selected connection. These are local organizational identities, not authentication. Connections do not automatically capture chats; agents must invoke `remember` or `observe`.

```python
from lmem import LightMem

admin = LightMem()
admin.register_user("alice")
admin.register_agent("coding", "alice")
admin.register_connection("agy", "alice", "coding", "antigravity", project_id="myapp", scope="project")

memory = LightMem(connection="agy", session="chat-123")
saved = memory.remember("Decision: use SQLite WAL", category="decision")
print(saved.context)
print(memory.context())
```

Python also exposes `register_harness`, `identities`, `connection_info`, `vault_status`, and `connect(mcp=True)`. Use different connection IDs for multiple harnesses while retaining the same user/project to share decisions.

---

## Single-File DB & Media Transfer Bundles (v0.5.0)

Transfer complete memory vaults between machines, servers, or workstations in a single compressed file—including all SQLite tables, full vector embeddings, knowledge graph links, and attached physical video, photo, audio, and PDF files:

```bash
# Export complete vault with clean SQLite checkpoint (VACUUM INTO) & all physical media
lmem bundle export -o my-vault.zip
lmem export --bundle -o my-vault.zip        # -o specifies output path for bundle export

# Inspect archive contents, record counts, and attachments without unzipping
lmem bundle info my-vault.zip

# Restore on a new machine with zero re-indexing or re-embedding overhead
lmem bundle import my-vault.zip --replace   # Explicit replace mode (default)
lmem bundle import my-vault.zip --merge     # Merge into existing memories without wiping
```

Bundles use the standard `.zip` (or `.lmbundle`) format containing:
- `memories.db`: Pristine SQLite database snapshot generated via `VACUUM INTO` (WAL flushed, defragmented).
- `manifest.json`: Verification manifest with schema version, record counts, SHA-256 digests, and media metadata.
- `attachments/`: Physical media files (4K videos, photos, voice notes, PDFs) extracted automatically to the target vault.

---

## Knowledge Graph Entities (v0.5.2)

When explicitly enabled with `lmem config --entity-extraction llm`, LightMem extracts structured knowledge graph entities (`name` → `value`) from ingested memories and dialogue turns. Entities are stored directly in SQLite (`entities` table), cross-referenced against canonical memories, deduplicated, and can answer exact attribute questions without running the QA model:

```bash
# Opt in: remembered text is sent to the configured LLM endpoint
lmem config --entity-extraction llm
# Store a memory and extract attributes such as pincode and city
lmem remember "I live at Flat 402, Sunshine Apts, Hyderabad with pincode 500081"

# Exact-match entity answer via entity_index
lmem answer "what is my pincode?"
# -> "500081" (reranker_used: entity_index)

# Inspect memory to see populated entities alongside wikilinks and attachments
lmem inspect <ID>

# Opt-out of LLM entity extraction for simple scratchpads or high-throughput batch writes
lmem remember "Temporary scratch note" --no-entities
```

In the Python SDK, entities are parsed into typed `EntityRecord` instances and available on each memory:

```python
from lmem import LightMem

lm = LightMem()
mem = lm.remember("PostgreSQL primary runs on port 5432")
for entity in mem.entities:
    print(f"Entity: {entity.name} = {entity.val}")
```

---

## Multimodal Embeddings & Direct Media Opening (v0.7.4)

- **Native Multimodal Embeddings (`embeddinggemma-2`)**: Configure `embeddinggemma-2` with Ollama (`lmem config --backend ollama --model embeddinggemma-2`) to embed images, video keyframes (extracted via `ffmpeg`), and audio clips natively via `/api/embed`.
- **Visual Image Query (`lmem recall --image`)**: Search stored memories by image similarity with optional text query: `lmem recall --image ./reference.png`.
- **Direct Media Opening (`--open`)**: Launch recalled or inspected attachments immediately in your system default viewer (`open` on macOS, `xdg-open` on Linux, `cmd /C start` on Windows): `lmem recall "diagram" --open` or `lmem inspect <ID> --open`.
- **Granular Format Tags & Attachment Dedup**: Storing attachments auto-tags both media kind (`#image`, `#video`) and lowercase extension (`#png`, `#mp4`), while alerting on duplicate SHA-256 blobs in batch or vault.
- **Unified Vault Telemetry Dashboard (`lmem status` / `vaults`)**: Inspect default and connection-scoped vaults, memory counts, and disk paths via `lmem status` (`lmem vaults`, `lmem identity status`, or Python `lm.vault_status()`).

---

## 🌐 Remote Backend Server & Multi-Device Access (`lmem serve`)

Run LightMem as a centralized, high-throughput memory backend on any device (home server, Mac mini, Linux VPS, Raspberry Pi, or local network) and connect from any client device:

```bash
# 1. Start the server (foreground)
lmem serve --host 0.0.0.0 --port 8080 --api-key "my-super-secret-key"

# 2. Run in background as a daemon (foreground/background preserve scope and session)
lmem serve -d --host 0.0.0.0 --port 8080 --api-key "my-super-secret-key"

# 3. Check daemon status & health
lmem serve --status

# 4. Stop background daemon cleanly
lmem serve --stop

# Auto-generates a secure random bearer token if --api-key is omitted
lmem serve -d --port 8080

# Or run without authentication for local trusted networks
lmem serve --port 8080 --no-auth
```

### Connect Client Devices

Any machine or agent with `lmem` installed can connect to the remote backend:

```bash
# Option A: Save in persistent configuration
lmem config --remote http://my-server.local:8080 --api-key "my-super-secret-key"

# Option B: Per-session or agent environment variables
export LIGHTMEM_REMOTE_URL="http://my-server.local:8080"
export LIGHTMEM_API_KEY="my-super-secret-key"

# Option C: Direct one-off command flag
lmem --remote http://my-server.local:8080 --api-key "my-super-secret-key" recall "project status"
```

Remote commands support `remember`, `recall`, `answer`, `list`, `inspect`, `forget`, `clear`, `link`, `unlink`, `related`, `graph`, `stats`, and `context`. Confidence, subsecond TTL, filters, Unicode titles, and inspection links are preserved. CLI `recall --precision` returns an answer using the server’s configured engine.

Unsupported vault commands (including import/export, conflicts, reindex, observe, and MCP launch) fail instead of operating locally. Local connection/scope selectors cannot be combined with remote commands. Visual recall, CLI attachment materialization/open flags, and attachment LLM extraction are rejected remotely; authenticated attachment bytes are available at `/v1/attachment`. `bundle info` is deliberately local file inspection. Explicit `--db`, `LIGHTMEM_DB`, or `--global` selects local storage over a saved remote endpoint.

Blank API keys are rejected; only `--no-auth` disables authentication. The health endpoint reports liveness/version without exposing the vault path. HTTP clear requires an explicit JSON object: `{"soft":true}` archives, and `{"soft":false}` permanently deletes. Configuration output redacts both API keys. Management endpoints require a loopback-bound server. Built-in HTTP does not encrypt traffic; use a trusted network or appropriate TLS infrastructure for remote operation.

---

## 📖 Embedded Local Documentation Server (`lmem docs`)

Serve complete technical documentation and the landing page 100% offline directly from the standalone binary without external web servers or background daemons:

```bash
# Launch documentation server and open system browser automatically
lmem docs                            # Aliases: lmem doc, lmem web

# Bind to a custom port or open landing page
lmem docs --port 8085
lmem docs --landing

# Run headlessly without launching the browser
lmem docs --no-browser
```

- **Endpoints**: `http://localhost:8080/` (landing page) and `http://localhost:8080/docs` (full technical docs).
- **Embedded Assets**: HTML, CSS, JavaScript, and icons are compiled directly into the `lmem` binary.
- **Zero Daemons**: Runs strictly in the foreground and terminates cleanly on `Ctrl+C`.

---

## 🔌 Model Context Protocol (MCP) Server

LightMem provides a native **Model Context Protocol (MCP)** stdio server (JSON-RPC 2.0). The process remains alive while the client is connected and may retain loaded models. Agents explicitly invoke its tools to store or retrieve memories.

### 1. One-Click Auto-Configuration
Configure MCP automatically across Claude Desktop, Cursor, and Antigravity:

```bash
lmem connect --mcp          # Auto-detects and writes config for all installed agent IDEs
lmem connect claude --mcp   # Configure Claude Desktop only
lmem connect cursor --mcp   # Configure Cursor only
```

### 2. Manual Configuration (`mcpServers`)
Add the following snippet to your editor's MCP configuration file (e.g., `claude_desktop_config.json` or `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "lightmem": {
      "command": "lmem",
      "args": ["mcp"]
    }
  }
}
```

### 3. Exposed MCP Tools

| Tool | Description |
| :--- | :--- |
| `remember` | Store persistent facts, preferences, decisions, or architectural notes with tags and optional files. |
| `recall` | Hybrid semantic + keyword retrieval with optional category, media kind, and multi-hop expansion. |
| `answer` | Neural Extractive QA: returns precise answer spans from vault memories. |
| `link` | Connect two memories intentionally in the knowledge graph with custom relations and weights. |
| `graph_related`| Multi-hop relationship traversal around a given memory node. |
| `conflicts` | Detect and review contradictory or duplicate memories. |
| `forget` | Deactivate or archive outdated memories safely. |
| `stats` | Inspect total memory count, vector distribution, and vault disk footprint. |

---

## ✦ Core CLI Reference

### 1. Remembering & Multimodal BLOB Storage
```bash
# Display Crimson Cloud HUD & live vault telemetry
lmem

# Store a memory (category is auto-inferred across 14 types if -t is omitted; LLM entities extracted only when enabled)
lmem remember "PostgreSQL 16 runs on port 5432" --title "Postgres Port" --tags "db,postgres"

# Store with explicit category, confidence, or skip entity extraction
lmem remember "Use AES-GCM-256 for vault encryption" -t decision --confidence 0.98
lmem remember "Ephemeral note" --no-entities                          # Skip automatic LLM entity extraction

# Store files, images, audio, or 4K videos in the isolated SQLite BLOB vault (0600 owner-only, SHA-256 verified)
lmem remember "Kubernetes ingress diagram" -f ./ingress.png             # Metadata-only default (dimensions, MIME, SHA-256)
lmem remember "Live wallpaper" -f ./wallpaper.mp4 --extract              # Opt-in: extract visual/file summary via configured LLM

# Export verified binary attachment(s) back to disk
lmem inspect <ID> --save-to ./exported/

# Materialize & print ONLY absolute file path(s) on disk (one per line)
lmem inspect <ID1> <ID2> --paths

# Open inspected attachment(s) immediately in default OS app
lmem inspect <ID> --open
```

### 2. Conversational Observation & Ingestion (`lmem observe`)
```bash
# Ingest dialogue or chat transcripts, extract atomic candidate memories, and review/confirm interactively
lmem observe "User: I prefer dark mode and JetBrains Mono.\nAssistant: Noted!" # Interactive confirmation [y/n/a/q]

# Pipe conversation transcripts via stdin or file
cat chat_transcript.txt | lmem observe
lmem observe conversation.txt --yes                   # Auto-confirm and save all extracted candidates
lmem observe conversation.txt --dry-run               # Preview extracted candidates without saving
lmem observe conversation.txt --dry-run --json        # Export candidates as JSON for agent inspection
lmem observe conversation.txt -r ollama:qwen2.5:3b    # LLM-assisted semantic extraction (aliases: observed, extract)
```

### 3. Hybrid Recall & Search
```bash
# Hybrid semantic + BM25 keyword recall
lmem recall "postgres port" --limit 5

# Recall with media filter (image | audio | video | document | any)
lmem recall "architecture diagram" -m image
lmem recall --image ./diagram.png                     # Visual image-to-memory search (embeddinggemma-2)
lmem recall "sharingan" -m video -l 1 --path          # Materialize & print ONLY verified file path(s)
lmem recall "sharingan" -l 1 --open                   # Recall & open attachment directly in default viewer

# Point-in-time and single-day temporal recall
lmem recall "who is kk" --date 2026-09-21              # Created or updated on that exact UTC date
lmem recall "postgres port" --as-of 2026-09-21         # Active state on or before that timestamp
```

### 4. Neural Extractive QA & Entity Index (`lmem answer`)
```bash
# Exact entity match that bypasses the answer model (reranker: entity_index)
lmem answer "what is my pincode"

# Local ONNX extractive QA (minilm-squad2 by default, local inference; latency depends on the host and model)
lmem answer "what is my pan card no"
lmem answer "what is my father name" -r tinyroberta-squad2

# Swappable Ollama cloud or local LLM reranker
lmem answer "who is kk" -r ollama:gemma4:31b-cloud
lmem answer "summarize sprint goals" -r ollama:qwen2.5:3b

# Return the highest-ranked retrieved memory
lmem answer "what port does postgres use?" -r top1
```

### 5. Knowledge Graph & Multi-Hop Traversal
```bash
# Obsidian-style wikilinks automatically link related memories
lmem remember "API Gateway routes to [[Postgres Port]]"

# Explicit directional knowledge graph edge with weight
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0

# Recall with 1-hop neighbor expansion
lmem recall "gateway" --multi-hop

# Multi-hop breadth-first graph traversal with hop-decay scoring
lmem related <ID> --hops 2

# Render ASCII/Unicode graph visualization centered on a node
lmem graph --focus <ID> --hops 2
```

### 6. Conflict Detection & Deduplication (`lmem conflicts`)
```bash
# Detect contradictory or duplicate memories via vector similarity + token overlap
lmem conflicts                        # Interactive step-by-step [y]es / [n]o / [a]ll / [q]uit
lmem conflicts --yes                  # Force-merge all detected conflicts automatically
lmem conflicts --min-similarity 0.85  # Adjust similarity threshold
lmem conflicts --yes -r ollama:qwen2.5:3b # Synthesize merged memory using LLM
```

### 7. Import, Export & Agent Skills
```bash
# Single-file compressed DB transfer bundle (.zip containing DB, all videos, photos, docs & vectors)
lmem bundle export -o backup.zip
lmem bundle info backup.zip
lmem bundle import backup.zip                  # Full vault restore + media extraction
lmem bundle import backup.zip --merge          # Merge into existing memories

# Import structured JSON, JSONL, OKF directory, MCP Graph, or Memanto memory.md
lmem import memories.json
lmem import ~/.memanto/on-prem/exports/memory.md

# Lossless backup export (Open Knowledge Format v2, JSON, or DB transfer bundle)
lmem export --okf -o backup.okf
lmem export --json -o backup.json
lmem export --bundle -o backup.zip

# Deploy LightMem skill to AI agent frameworks
lmem connect all                      # Antigravity, Codex, Hermes, Cursor, Claude
lmem connect --mcp                    # Auto-configure MCP in Claude Desktop, Cursor, Antigravity

# Vault telemetry, connection overview & offline embedded docs
lmem stats
lmem status                                           # Visual dashboard of all managed vaults & connections
lmem docs                                             # Serve offline docs at http://localhost:8080/docs
```

---

## Everyday terminal setup

LightMem uses ordinary commands and short prompts. New local setups use BGE small
search, MiniLM answers, and entity extraction off.

```sh
lmem setup                         # Enter accepts local defaults; preview before saving
lmem config --edit                 # Change one group through numbered prompts
lmem doctor                        # Read-only checks; no downloads or network requests
lmem config --preset demo --yes     # Try hash retrieval without model downloads
lmem config --preset local --dry-run # Preview shared settings and selected-vault migration
lmem config --answer-model top1     # Clear answer-model spelling; --reranker still works
```

`--embedding-model` aliases `config --model` for Ollama search models;
`--answer-model` aliases `config --reranker` and `setup --model`. Existing flag
meanings are preserved. Settings are shared under the configuration root; a model
change can require approval to reindex the selected vault. Other vaults may need
reindexing separately. Entity extraction is an explicit `--entity-extraction llm`
opt-in; remembered text then goes to the configured model service.

Enable Tab completion for the current shell:

```sh
source <(lmem completions bash)     # Bash
source <(lmem completions zsh)      # Zsh, after compinit
lmem completions fish | source     # Fish
```

Bash, Zsh, and Fish complete registered connection IDs using a read-only lookup.
Completion and `doctor` do not create a configuration directory or vault. Doctor
separates untested providers from index readiness; run `lmem config --test` for an
explicit embedding check (which can prepare/download models). `--edit` requires a
terminal. Scripts use explicit flags, `--dry-run`, `--yes` where needed, and
`--json`; they never enter the new prompts.

See [terminal command guide](docs/terminal-commands.md) for setup and override details.

## ⇄ Switching Rerankers & Embedding Models

LightMem supports three swappable reranker backends for `lmem answer`, `lmem recall --precision`, and `lmem conflicts`:

| Answer engine | Set default | Behavior |
| :--- | :--- | :--- |
| `minilm-squad2` (default) | `lmem config --reranker minilm-squad2` | Local extractive QA over retrieved memories |
| `tinyroberta-squad2` | `lmem config --reranker tinyroberta-squad2` | Alternative local extractive QA model |
| `ollama:<model>` | `lmem config --reranker ollama:qwen2.5:3b` | Uses the configured local or remote endpoint |
| `top1` | `lmem config --reranker top1` | Returns the best retrieved memory without an additional answer model |

Use `answer -r MODE` for a per-query override. Latency and RAM depend on model initialization, corpus, host, and runtime mode; benchmark your actual setup rather than comparing mixed cold/warm estimates.

```bash
# 1. Interactive Setup Wizard (prompted automatically during install)
lmem setup                       # Terminal choices and a preview before saving
lmem setup --preset demo --dry-run # No-download demo preview
lmem setup --preset demo --yes     # Save reviewed hash/top1 settings
lmem setup --preset ollama-local --entity-extraction off --yes

# 2. Inspect active configuration and presets
lmem config                      # Formatted tree of settings
lmem config --json               # Machine-readable JSON output
lmem config --preset local --dry-run # Preview shared settings and selected-vault migration
lmem config --test               # Test saved embeddings without saving a memory
lmem context                    # Show selected vault and connection/scope

# 3. Local model preset (ONNX search + QA; entity extraction off)
lmem config --preset local
lmem config --clear-remote       # Also select local CLI routing if a remote server was configured
lmem config --download all       # Pre-cache ONNX models locally for offline operation

# 4. Preset: Local Embeddings + Ollama Cloud Model Reranker
lmem config --preset ollama-cloud

# 5. Preset: Local Embeddings + Ollama Localhost LLM Reranker
lmem config --preset ollama-local

# 6. Custom Ollama Host or Model
lmem config --url http://localhost:11434 --reranker ollama:qwen2.5:3b
lmem config --entity-extraction off # Configure separately from search and answers
# Set OLLAMA_API_KEY in your environment for credentials, or save a key with --ollama-api-key.

# 7. Dynamic Per-Query Override (without changing persistent config)
lmem answer "what is my api token" -r minilm-squad2
lmem answer "summarize sprint goals" -r ollama:gemma4:31b-cloud
```

---

## ⇄ Nearest-Neighbor Conflict & Duplicate Resolution (`lmem conflicts`)

Instead of brittle hardcoded slot regexes, `lmem conflicts` scans active memories by finding each memory's **closest vector neighbor** (`cosine_similarity`) combined with **meaningful token overlap**:

1. **Exact & Near-Duplicate Detection**: Memories with high vector similarity and token overlap are paired and sorted by similarity descending.
2. **Timestamp Transparency**: Every pair displays exact UTC creation timestamps (`[OLDER · YYYY-MM-DD HH:MM:SS UTC]` vs `[NEWER · YYYY-MM-DD HH:MM:SS UTC]`) alongside similarity `%` and overlap `%`.
3. **Interactive or Batch Resolution**:
   - In an interactive terminal, `lmem conflicts` prompts on each pair:
     `Merge these memories? [y]es / [n]o (next) / [a]ll (--yes) / [q]uit`
   - Passing `--yes` (`-y`, alias `--resolve`) merges all pairs automatically.
4. **Review the merge behavior**: Top‑1/default merging generally keeps the newer record’s complete content; local ONNX has a narrow richer-subset exception. Ollama is prompted to preserve non-conflicting older details, but the result still needs review. Merging unions tags/attachments, refreshes the survivor’s vector, and expires the older record. Similarity is a candidate signal, not proof of a contradiction.

---

## ◷ Temporal Filtering (`--as-of` vs `--date`)

- **`--as-of <YYYY-MM-DD|RFC3339>`**: Historical visibility filter — returns memories created on or before that timestamp that were still active at that moment.
This filters record visibility; it is not a complete history of prior content revisions.

- **`--date <YYYY-MM-DD>`**: Single-day filter — returns memories created or updated on that exact UTC calendar day (supported in `lmem recall`, `lmem list`, and `lmem answer`).

---

## ◈ Clean Uninstall

To remove the binary, shell completions, and shell configuration hooks (preserves your memories database at `~/.lightmem/memories.db` by default):

```bash
# Option 1: Native CLI command
lmem uninstall

# Option 2: One-line script (macOS & Linux)
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/uninstall.sh | sh

# Option 3: Windows PowerShell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/uninstall.ps1 | iex

# To also purge your database and downloaded models:
lmem uninstall --purge-data
```

---

## ⌖ Development & Testing

```bash
# Fast offline test suite (skips ONNX weight download)
LIGHTMEM_QA_DISABLE=1 cargo test --locked

# Full test suite including live ONNX Extractive QA inference
cargo test --locked

# Formatting & strict clippy lint check
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

---

## 📄 License

LightMem is released under the [MIT License](LICENSE). Local file permissions and a `password` category do not provide encryption at rest; protect vaults and backup bundles accordingly.
