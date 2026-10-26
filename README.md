# ❖ LightMem (`lmem`)

**Ultra-fast, local AI agent persistent memory engine and native Model Context Protocol (MCP) server, written in Rust with SQLite WAL, ONNX Runtime (`fastembed` + SQuAD-2.0 Extractive QA), and swappable Ollama models.**

[![GitHub Release](https://img.shields.io/github/v/release/krishnakanthpathi/lightmem?color=crimson&logo=github)](https://github.com/krishnakanthpathi/lightmem/releases)
[![PyPI version](https://img.shields.io/pypi/v/lmem.svg?color=blue&logo=pypi&logoColor=white)](https://pypi.org/project/lmem/)
[![Crates.io](https://img.shields.io/crates/v/lightmem.svg?color=orange&logo=rust)](https://crates.io/crates/lightmem)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/krishnakanthpathi/lightmem)
[![Privacy](https://img.shields.io/badge/privacy-100%25%20local-success.svg)](https://github.com/krishnakanthpathi/lightmem)

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **local ONNX Extractive QA (`minilm-squad2` / `tinyroberta-squad2`) & swappable Ollama rerankers** (`ollama:<model>`), **immutable SHA-256-verified multimodal BLOB attachments** (images, audio, video, documents), **Obsidian-grade `[[wikilinks]]` & multi-hop knowledge graph traversal**, **nearest-neighbor vector + overlap conflict merging**, **temporal point-in-time (`--as-of`) & single-day (`--date`) queries**, **14-category auto-inference**, and a **zero-overhead Model Context Protocol (MCP) server** for AI agent IDEs.

---

## ◈ Table of Contents

- [Why LightMem?](#-why-lightmem)
- [Performance & Comparison](#-performance--comparison)
- [Quick Install](#-quick-install-macos-linux--windows)
- [Python SDK Quickstart](#-python-sdk-quickstart)
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

- **Sub-10ms Startup & Sub-Millisecond Retrieval**: Pure compiled Rust binary with direct SQLite C-bindings and memory-mapped indices.
- **Zero Background Daemons**: 0% idle CPU and 0% idle RAM. Activates only when invoked by shell, Python SDK, or MCP JSON-RPC.
- **100% Private & Offline**: No telemetry, no external database dependencies, and no cloud sync required.
- **Neural Extractive QA**: Extracts pinpoint answers (under 20 tokens) directly from stored memories instead of flooding LLM context windows.
- **Multimodal BLOB Vault**: Secure `0600` file storage for images, diagrams, audio, and 4K video with SHA-256 integrity checks and instant path materialization (`--path`).
- **Graph Multi-Hop Traversal**: Connect memories with Obsidian-style `[[wikilinks]]` or explicit directional edges and navigate relationship trees.
- **Smart Conflict Merging**: Nearest-neighbor cosine similarity and token overlap scanner that finds and synthesizes conflicting facts into updated records.

---

## 📊 Performance & Comparison

| Feature / Metric | **LightMem (`lmem`)** | **Traditional Cloud / Daemon Memory** | **Standard Vector Databases** |
| :--- | :--- | :--- | :--- |
| **Runtime Language** | **Compiled Rust** | Python / Node.js | Python / Go / Java |
| **Idle Memory (RAM)** | **0 MB** (no persistent daemon) | 250 MB – 1.2 GB | 500 MB – 2 GB |
| **CLI Startup Latency** | **~8 ms** | 450 ms – 1.8 s | N/A (requires daemon) |
| **Exact Fact Extraction** | **Local ONNX QA (`minilm-squad2`)** | LLM Re-prompting | None (returns raw chunks) |
| **Multimodal Storage** | **Built-in SHA-256 BLOB Vault** | Separate S3 / MinIO | Separate Object Store |
| **Knowledge Graph** | **`[[wikilinks]]` + Multi-hop BFS** | Often None or Extra Graph DB | Vector-only |
| **Protocol Support** | **Native MCP stdio + Python SDK + CLI** | Custom REST API | Custom REST / gRPC |
| **Privacy / Cloud Sync** | **100% Local (Air-gapped ready)** | Cloud-bound / External Server | Self-hosted or Cloud |

---

## ◈ Quick Install (macOS, Linux & Windows)

### Python (Precompiled Native Wheel — Instant sub-10ms startup)
```bash
pip install -U lmem
```

### Universal Shell Installer (macOS & Linux)
```bash
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex
```

### Build from Source (Rust / Cargo)
```bash
git clone https://github.com/krishnakanthpathi/lightmem.git
cd lightmem
cargo install --path . --force
```

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
diagram = lm.remember(
    "API Gateway routes to [[PostgreSQL Primary Config]]",
    files=["./architecture-diagram.png"],
    tags=["architecture", "gateway"],
    extract=False,  # Set True to extract visual summary via configured LLM
)

# 3. Hybrid Recall (FTS5 BM25 + Vector Cosine RRF + Graph Multi-Hop)
results = lm.recall("postgres port", limit=5, multi_hop=True)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# Materialize verified attachment file paths directly on disk
image_paths = lm.recall("gateway", media="image", limit=1, path=True)

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

Python also exposes `register_harness`, `identities`, `connection_info`, and `connect(mcp=True)`. Use different connection IDs for multiple harnesses while retaining the same user/project to share decisions.

## 🔌 Model Context Protocol (MCP) Server

LightMem features a native, zero-dependency **Model Context Protocol (MCP)** stdio server (JSON-RPC 2.0). It operates on-demand without background daemons and consumes 0% idle CPU.

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

# Store a memory (category is auto-inferred across 14 types if -t is omitted)
lmem remember "PostgreSQL 16 runs on port 5432" --title "Postgres Port" --tags "db,postgres"

# Store with explicit category and confidence
lmem remember "Use AES-GCM-256 for vault encryption" -t decision --confidence 0.98

# Store files, images, audio, or 4K videos in the isolated SQLite BLOB vault (0600 owner-only, SHA-256 verified)
lmem remember "Kubernetes ingress diagram" -f ./ingress.png             # Metadata-only default (dimensions, MIME, SHA-256)
lmem remember "Live wallpaper" -f ./wallpaper.mp4 --extract              # Opt-in: extract visual/file summary via configured LLM

# Export verified binary attachment(s) back to disk
lmem inspect <ID> --save-to ./exported/

# Materialize & print ONLY absolute file path(s) on disk (one per line)
lmem inspect <ID1> <ID2> --paths
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
lmem recall "sharingan" -m video -l 1 --path          # Materialize & print ONLY verified file path(s)

# Point-in-time and single-day temporal recall
lmem recall "who is kk" --date 2026-09-21              # Created or updated on that exact UTC date
lmem recall "postgres port" --as-of 2026-09-21         # Active state on or before that timestamp
```

### 4. Neural Extractive QA (`lmem answer`)
```bash
# Local ONNX extractive QA (minilm-squad2 by default, sub-600ms latency, zero API costs)
lmem answer "what is my pan card no"
lmem answer "what is my father name" -r tinyroberta-squad2

# Swappable Ollama cloud or local LLM reranker
lmem answer "who is kk" -r ollama:gemma4:31b-cloud
lmem answer "summarize sprint goals" -r ollama:qwen2.5:3b

# Instant zero-overhead rank-1 vector answer
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

# Vault telemetry & disk storage statistics
lmem stats
```

---

## ⇄ Switching Rerankers & Embedding Models

LightMem supports three swappable reranker backends for `lmem answer`, `lmem recall --precision`, and `lmem conflicts`:

| Reranker | Command to Set Default | Per-Query Flag (`-r`) | Peak RAM | Typical Latency |
| :--- | :--- | :--- | :--- | :--- |
| **`minilm-squad2`** *(default)* | `lmem config --reranker minilm-squad2` | `-r minilm-squad2` | ~358 MB | ~600 ms |
| **`tinyroberta-squad2`** | `lmem config --reranker tinyroberta-squad2` | `-r tinyroberta-squad2` | ~669 MB | ~1.2 s |
| **`ollama:<model>`** *(local or cloud)* | `lmem config --reranker ollama:qwen2.5:3b` | `-r ollama:gemma4:31b-cloud` | ~207 MB (`lmem`) | ~1–11 s |
| **`top1`** *(0ms vector rank-1)* | `lmem config --reranker top1` | `-r top1` | ~205 MB | ~280 ms |

```bash
# 1. Interactive Setup Wizard (prompted automatically during install)
lmem setup                       # Interactive CLI wizard: Local Ollama, Ollama Cloud, Custom, or Offline
lmem setup --preset ollama-local # One-shot preset configuration

# 2. Inspect active configuration and presets
lmem config                      # Formatted tree of settings
lmem config --json               # Machine-readable JSON output

# 3. Preset: 100% Offline Local Mode (ONNX bge-small + minilm-squad2 QA)
lmem config --preset local
lmem config --download all       # Pre-cache ONNX models locally for offline operation

# 4. Preset: Local Embeddings + Ollama Cloud Model Reranker
lmem config --preset ollama-cloud

# 5. Preset: Local Embeddings + Ollama Localhost LLM Reranker
lmem config --preset ollama-local

# 6. Custom Ollama Host or Model
lmem config --url http://localhost:11434 --reranker ollama:qwen2.5:3b

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
4. **Reranker-Guided Merging**: When merging, the active reranker (`OnnxQaReranker`, `OllamaReranker`, or `Top1Reranker`) synthesizes the merged record: prioritizing the `NEWER` memory's updated facts while preserving non-conflicting clauses from `OLDER`, unioning tags case-insensitively, keeping max confidence, re-embedding the survivor, and expiring the `OLDER` record.

---

## ◷ Temporal Filtering (`--as-of` vs `--date`)

- **`--as-of <YYYY-MM-DD|RFC3339>`**: Cumulative point-in-time snapshot — returns memories created on or before that timestamp that were still active at that moment.
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

LightMem is released under the [MIT License](LICENSE).
