# ❖ LightMem (`lmem`)

**Ultra-fast local-first persistent memory engine for AI agents: native Rust CLI, Python SDK, MCP server, and SQLite storage with zero mandatory cloud dependencies.**

[![GitHub Release](https://img.shields.io/github/v/release/krishnakanthpathi/lightmem?color=crimson&logo=github)](https://github.com/krishnakanthpathi/lightmem/releases)
[![PyPI version](https://img.shields.io/pypi/v/lmem.svg?color=blue&logo=pypi&logoColor=white)](https://pypi.org/project/lmem/)
[![Crates.io](https://img.shields.io/crates/v/lightmem.svg?color=orange&logo=rust)](https://crates.io/crates/lightmem)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/krishnakanthpathi/lightmem)
[![Storage: Local-First](https://img.shields.io/badge/storage-local%20first-success.svg)](https://github.com/krishnakanthpathi/lightmem)

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector embeddings** (`bge-small`), **local ONNX Extractive QA** (`minilm-squad2`) or **swappable OpenAI/Ollama LLM engines**, **secure `0600` SHA-256 multimodal file attachments** (images, audio, video, PDFs), **knowledge graph `[[wikilinks]]` with multi-hop traversal**, **nearest-neighbor conflict resolution**, and a **native Model Context Protocol (MCP) server** for AI agent IDEs.

---

## ◈ Table of Contents

- [Why LightMem?](#-why-lightmem)
- [Quick Installation](#-quick-installation)
- [Python SDK Quickstart](#-python-sdk-quickstart)
- [Core CLI Quickstart](#-core-cli-quickstart)
- [Engine Configuration & Backends](#-engine-configuration--backends)
- [Multimodal BLOB Vault](#-multimodal-blob-vault)
- [Knowledge Graph & Multi-Hop Traversal](#-knowledge-graph--multi-hop-traversal)
- [Extractive QA & Pinpoint Answers](#-extractive-qa--pinpoint-answers)
- [Single-File DB & Media Transfer Bundles](#-single-file-db--media-transfer-bundles)
- [Remote Server & Multi-Device Sync](#-remote-server--multi-device-sync)
- [Model Context Protocol (MCP) Server](#-model-context-protocol-mcp-server)
- [Embedded Offline Documentation Server](#-embedded-offline-documentation-server)
- [Memory Categories Reference](#-memory-categories-reference)
- [Development & Testing](#-development--testing)
- [License](#-license)

---

## ⚡ Why LightMem?

- **Zero Cloud Lock-in**: 100% offline by default. SQLite WAL database stays on your disk.
- **Sub-Millisecond Search**: Hybrid lexical (FTS5 BM25) + semantic vector cosine reciprocal rank fusion (RRF).
- **Neural Extractive QA**: Extracts exact fact spans directly from memory without filling LLM context windows.
- **Multimodal BLOB Storage**: Stores diagrams, photos, audio clips, and 4K videos in an isolated `0600` SQLite BLOB vault with instant path materialization (`--path`) and direct viewing (`--open`).
- **Graph Multi-Hop Traversal**: Connect memories with Obsidian-style `[[wikilinks]]` or explicit weighted edges.
- **Nearest-Neighbor Conflict Merging**: Automatically identifies overlapping or contradictory memories via vector similarity and token overlap.
- **Single-File Bundles**: Export and restore complete memory vaults (database + full embeddings + all attached physical files) in a single compressed `.zip` bundle.

---

## ◈ Quick Installation

### Universal Shell Installer (macOS & Linux)
```bash
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh
```

### Python Package (with native precompiled binary)
```bash
pip install -U lmem
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex
```

### Cargo / Build from Source
```bash
cargo install --path . --force
```

---

## 🐍 Python SDK Quickstart

```python
from lmem import LightMem

# Initialize local vault (defaults to ~/.lightmem/memories.db)
lm = LightMem()

# 1. Store a memory (category auto-infers across 13 types if omitted)
memory = lm.remember(
    "PostgreSQL primary runs on port 5432 with WAL archiving enabled",
    category="fact",
    title="PostgreSQL Primary Config",
    tags=["postgres", "database", "production"],
    confidence=0.95,
)
print(f"Stored memory ID: {memory.id}")

# 2. Hybrid Recall (FTS5 BM25 + Vector Cosine RRF)
results = lm.recall("postgres port", limit=5)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# 3. Neural Extractive QA (extracts exact fact span)
answer = lm.answer("what port does postgresql use?")
print(f"Extracted Answer: {answer.answer}")      # e.g. "5432"

# 4. Attach and recall physical files
from pathlib import Path
notes = Path("ingress-diagram.png")
if notes.exists():
    lm.remember("Kubernetes ingress diagram", files=[notes])
    paths = lm.recall("ingress", media="image", limit=1, path=True)
    print(f"File path: {paths}")

# 5. Graph linking and multi-hop traversal
lm.remember("API Gateway routes to [[PostgreSQL Primary Config]]")
related = lm.related(memory.id, hops=2)

# 6. Single-file DB & media transfer bundle
lm.bundle_export("my-vault.zip")
lm.bundle_import("my-vault.zip", mode="replace")  # or mode="merge"
```

---

## ✦ Core CLI Quickstart

```bash
# 1. Store a memory
lmem remember "PostgreSQL primary runs on port 5432 with WAL archiving enabled" \
  -t fact \
  --title "Postgres Port" \
  --tags "postgres,database,production" \
  --confidence 0.95 \
  --supersede

# 2. Recall memories (hybrid vector + full-text search)
lmem recall "postgres port" --limit 5

# 3. Neural Extractive QA
lmem answer "what port does postgres use?"

# 4. Inspect memory record, entities, and links
lmem inspect <MEMORY_ID>

# 5. Interactive conflict & duplicate review
lmem conflicts
lmem conflicts --yes                               # Auto-merge detected duplicates

# 6. Lifecycle & deletion
lmem forget <MEMORY_ID>                            # Soft-retire (status -> expired)
lmem forget <MEMORY_ID> --hard                     # Permanent purge (DB + vector + FTS)
```

---

## ⚙ Engine Configuration & Backends

LightMem uses unified, straightforward configuration settings stored in `~/.lightmem/config.json`.

```bash
# View active settings
lmem config

# Quick Setup Presets
lmem config --preset local        # 100% offline local ONNX search + answers (zero setup)
lmem config --preset ollama       # Local ONNX embeddings + local Ollama LLM
lmem config --preset openai       # Local ONNX embeddings + OpenAI (gpt-4o-mini)
lmem config --preset demo         # Deterministic hash search (no model downloads)

# Custom Configuration
lmem config --backend onnx|ollama|hash        # Set search embedding backend
lmem config --embedding-model <model>         # Set vector embedding model (e.g. bge-small)
lmem config --openai-model <model>            # Set chat/QA model (e.g. gpt-4o-mini, ollama:qwen2.5:0.5b)
lmem config --entity-extraction on|off        # Toggle entity extraction
lmem config --endpoint <url>                  # Set OpenAI-compatible endpoint URL
lmem config --api-key <key>                   # Set API key for model service
lmem config reload                            # Reload settings directly from config.json
lmem doctor                                   # Run diagnostic health check
```

### Example `~/.lightmem/config.json`
```json
{
  "backend": "onnx",
  "embedding_model": "bge-small",
  "openai_model": "ollama:qwen2.5:0.5b",
  "openai_endpoint": "http://localhost:11434",
  "openai_api_key": "optional-key-here",
  "entity_extraction": "on"
}
```

---

## 📎 Multimodal BLOB Vault

LightMem includes an isolated `0600` SQLite BLOB vault for securely storing binary files with SHA-256 integrity verification:

```bash
# Store images, diagrams, audio, or video
lmem remember "Architecture overview diagram" -f ./arch.png
lmem remember "Team standup voice note" -f ./standup.wav
lmem remember "Product demo" -f ./demo.mp4 -f ./specs.pdf

# Materialize and print absolute path(s) on disk
lmem inspect <MEMORY_ID> --path
lmem inspect <ID1> <ID2> --paths

# Open attachment in default OS viewer
lmem inspect <MEMORY_ID> --open
lmem recall "architecture" -m image -l 1 --open

# Filter recall by media kind
lmem recall "diagram" --media image      # image | audio | video | document | any
```

---

## 🕸 Knowledge Graph & Multi-Hop Traversal

```bash
# Obsidian-style wikilinks automatically link related memories
lmem remember "API Gateway routes to [[Postgres Port]]"

# Explicit directional edges with custom relationship & weight
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0
lmem unlink <SRC_ID> <DST_ID>

# Multi-hop graph traversal
lmem related <MEMORY_ID> --hops 2

# ASCII/Unicode terminal graph visualization
lmem graph --focus <MEMORY_ID> --hops 2
```

---

## 🎯 Extractive QA & Pinpoint Answers

`lmem answer` returns exact fact spans rather than generating verbose hallucinations:

```bash
# Exact entity match from memory index
lmem answer "what is my pincode"

# Local ONNX extractive QA (minilm-squad2 by default)
lmem answer "what port does postgresql use"

# Swappable LLM QA (Ollama, OpenAI, Gemini, Groq, etc.)
lmem answer "summarize deployment steps" -m ollama:qwen2.5:0.5b
```

---

## 📦 Single-File DB & Media Transfer Bundles

Transfer or backup complete memory vaults between machines in a single compressed archive without re-embedding:

```bash
# Export complete vault (.zip with DB, vector embeddings & all physical media)
lmem bundle export -o my-vault.zip

# Inspect bundle manifest and file counts without extracting
lmem bundle info my-vault.zip

# Restore on another machine
lmem bundle import my-vault.zip               # Replace mode
lmem bundle import my-vault.zip --merge       # Merge into existing memories
```

---

## 🌐 Remote Server & Multi-Device Sync

Run LightMem as a centralized memory backend for all your workstations and agent runners:

```bash
# Start server in foreground
lmem serve --host 0.0.0.0 --port 8080 --api-key "secret-token"

# Run as background daemon
lmem serve -d --host 0.0.0.0 --port 8080 --api-key "secret-token"
lmem serve --status
lmem serve --stop

# Connect client CLI to remote instance
lmem remote http://my-server:8080 --api-key "secret-token"
lmem recall "project decisions"

# Revert to local mode
lmem local
```

---

## 📜 Command History & Audit Trail

LightMem automatically tracks executed CLI commands in an isolated local SQLite database (`~/.lightmem/history.db`), recording timestamps, execution status, duration, and arguments with credentials securely redacted:

```bash
# View recent command execution history
lmem history

# Search history for specific terms
lmem history "answer"

# Filter by execution status with limit
lmem history --status error -n 10
lmem history --status success

# Output structured JSON (ideal for agent automation)
lmem history -n 5 --json

# Clear history database
lmem history --clear
```

---

## 🔌 Model Context Protocol (MCP) Server

LightMem natively implements the **Model Context Protocol (MCP)** (JSON-RPC 2.0 stdio) for direct integration into AI agent environments:

### Automatic One-Click Configuration
```bash
lmem connect --mcp          # Configures Claude Desktop, Cursor, and Antigravity
lmem connect claude --mcp   # Configure Claude Desktop only
lmem connect cursor --mcp   # Configure Cursor only
```

### Manual Configuration (`claude_desktop_config.json` / `.cursor/mcp.json`)
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

### Exposed MCP Tools
| Tool | Description |
| :--- | :--- |
| `remember` | Store persistent facts, preferences, decisions, or architectural notes with tags and files. |
| `recall` | Hybrid semantic + keyword retrieval with optional category, media kind, and multi-hop expansion. |
| `answer` | Neural Extractive QA: returns precise answer spans from vault memories. |
| `link` | Connect two memories in the knowledge graph with custom relations and weights. |
| `graph_related` | Multi-hop relationship traversal around a given memory node. |
| `conflicts` | Detect and review contradictory or duplicate memories. |
| `forget` | Deactivate or archive outdated memories safely. |
| `stats` | Inspect total memory count, vector distribution, and vault disk footprint. |

---

## 📖 Embedded Offline Documentation Server

Browse full technical documentation offline with zero external web dependencies:

```bash
# Launch documentation server and open system browser
lmem docs

# Custom port or headless mode
lmem docs --port 8085 --no-browser
```

---

## 📋 Memory Categories Reference

| Category | When to Use | Recommended Confidence | Typical Provenance |
| :--- | :--- | :--- | :--- |
| `fact` | Verified system facts, ports, URLs, personal/identity details | `0.90 - 1.00` | `validated` / `explicit_statement` |
| `decision` | Architectural choices, stack selections, design trade-offs | `0.90 - 1.00` | `explicit_statement` |
| `instruction` | Standing user rules, coding conventions, mandatory constraints | `0.95 - 1.00` | `explicit_statement` |
| `preference` | User workflow, UI/UX, editor, or communication preferences | `0.85 - 1.00` | `explicit_statement` |
| `learning` | Post-debugging insights, workarounds, root causes | `0.85 - 0.95` | `validated` / `corrected` |
| `goal` | Project milestones, OKRs, long-term targets | `0.85 - 1.00` | `explicit_statement` |
| `commitment` | Action items, TODOs, deliverables | `0.90 - 1.00` | `explicit_statement` |
| `artifact` | File paths, schemas, build outputs | `0.90 - 1.00` | `validated` |
| `event` | Incidents, deployments, migrations | `0.85 - 0.95` | `validated` |
| `relationship` | Ownership, team mapping, service dependencies | `0.85 - 0.95` | `explicit_statement` |
| `error` | Crash signatures, stack traces, regression postmortems | `0.90 - 1.00` | `validated` / `corrected` |
| `context` | Current workspace state, active branch, session notes | `0.80 - 0.95` | `validated` |
| `password` | Credentials, DSNs with embedded auth, secret references | `0.95 - 1.00` | `explicit_statement` |

---

## ⌖ Development & Testing

```bash
# Fast offline test suite
LIGHTMEM_QA_DISABLE=1 cargo test --locked

# Full test suite (including live ONNX QA inference)
cargo test --locked

# Strict formatting & clippy lint checks
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings

# Python SDK tests
python3 -m unittest discover -s python/tests -v
```

---

## 📄 License

LightMem is open source and released under the [MIT License](LICENSE).
