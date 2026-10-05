# ❖ LightMem (`lmem`)

**Local agent memory engine and CLI, written in Rust with SQLite, ONNX Runtime (`fastembed` + SQuAD-2.0 Extractive QA), and swappable Ollama models.**

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **local ONNX Extractive QA (`minilm-squad2` / `tinyroberta-squad2`) & swappable Ollama rerankers** (`ollama:<model>`), **nearest-neighbor vector + overlap conflict merging**, **temporal point-in-time (`--as-of`) & single-day (`--date`) queries**, **14-category auto-inference**, and **multi-format JSON / JSONL / OKF / MCP Knowledge Graph / Memanto Markdown ingestion**.

---

## ◈ One-Line Install (macOS, Linux & Windows)

Install the prebuilt `lmem` `v0.2.0` binary and pre-cache the local ONNX embedding (`bge-small`) and Extractive QA (`minilm-squad2`) models with a single command:

### macOS & Linux
```bash
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/neural-reranker/install.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/neural-reranker/install.ps1 | iex
```

### Build from Source (Rust / Cargo)
```bash
git clone -b neural-reranker https://github.com/krishnakanthpathi/lightmem.git
cd lightmem
cargo install --path . --force
```

---

## ◫ Architecture (MVC / MVP)

```text
lightmem/
├── Cargo.toml
├── install.sh                      # Universal macOS / Linux installer (v0.2.0)
├── src/
│   ├── lib.rs                      # Library root & public re-exports
│   ├── main.rs                     # CLI router (Clap -> Controller -> View)
│   ├── models/                     # Domain Entities & Value Objects
│   │   ├── mod.rs
│   │   ├── memory.rs               # 14-category MemoryType::infer, MemoryRecord, MemoryConflict
│   │   ├── config.rs               # LightMemConfig & reranker/embedding identities
│   │   └── stats.rs                # StorageStats telemetry model
│   ├── repositories/               # Data Access Layer
│   │   ├── mod.rs
│   │   └── sqlite_repo.rs          # SQLite WAL + FTS5 + vector blob persistence
│   ├── services/                   # Business & Domain Engines
│   │   ├── mod.rs
│   │   ├── embeddings.rs           # FastEmbed ONNX (bge-small, minilm, nomic), Ollama, & Hash
│   │   ├── search.rs               # Hybrid RRF (BM25 + Cosine) + acronym/lexical boosts
│   │   ├── reranker.rs             # OnnxQaReranker (SQuAD-2.0), OllamaReranker, & Top1Reranker
│   │   ├── importer.rs             # JSON / JSONL / OKF / MCP Graph / Memanto Markdown importer
│   │   └── exporter.rs             # Open Knowledge Format (OKF v2) & JSON backup exporter
│   ├── controllers/                # Application Orchestration
│   │   ├── mod.rs
│   │   └── memory_controller.rs    # MemoryController (LightMem) with OnceLock lazy ONNX init
│   └── views/                      # Presentation Layer
│       ├── mod.rs
│       └── cli_view.rs             # 24-bit TrueColor Crimson Cloud HUD, vector icons, & JSON output
└── tests/
    ├── integration_test.rs
    ├── integrity_and_migration.rs
    ├── cli_migration.rs
    └── review_regressions.rs
```

---

## ✦ Core CLI Usage

```bash
# Display Crimson Cloud HUD & live vault telemetry
lmem

# Store a memory (category is auto-inferred across 14 types if -t is omitted)
lmem remember "PostgreSQL 16 runs on port 5432" --title "Postgres Port" --tags "db,postgres"

# Store a memory and automatically merge/supersede any conflicting older memory
lmem remember "PostgreSQL 16 runs on port 6432" --title "Postgres Port" --supersede

# Hybrid semantic + keyword recall (with optional --as-of or single-day --date filter)
lmem recall "postgres port" --limit 5
lmem recall "who is kk" --date 2026-09-21
lmem recall "postgres port" --as-of 2026-09-21

# Extractive Question Answering (uses local ONNX minilm-squad2 by default)
lmem answer "what is my pan card no"
lmem answer "what is my father name" -r tinyroberta-squad2
lmem answer "who is kk" -r ollama:gemma4:31b-cloud
lmem answer "what port does postgres use?" -r top1

# Paginated chronological list (supports --date and --as-of)
lmem list --page 1 --limit 20
lmem list --date 2026-09-21
lmem list --offset 20 --limit 20 -t decision

# Detect and merge duplicate or conflicting memories using vector similarity + reranker
lmem conflicts                        # Interactive step-by-step [y]es / [n]o / [a]ll / [q]uit
lmem conflicts --yes                  # Force-merge all detected conflicts
lmem conflicts --yes -r ollama:qwen2.5:3b
lmem conflicts --min-similarity 0.85

# Import structured or unstructured JSON, JSONL, OKF directory, MCP Graph, or Memanto memory.md
lmem import memories.json
lmem import ~/.memanto/on-prem/exports/memory.md

# Export to Open Knowledge Format (OKF v2) or lossless JSON backup
lmem export --okf -o backup.okf
lmem export --json -o backup.json

# Vault storage statistics & category distribution
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
# Pre-download ONNX models for offline usage
lmem config --download minilm-squad2
lmem config --download tinyroberta-squad2
lmem config --download all

# Switch embedding backend (prompts for safe atomic re-indexing)
lmem config --backend onnx --onnx-model bge-small
lmem config --backend ollama --model nomic-embed-text --url http://localhost:11434
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
4. **Reranker-Guided Merging**: When merging, the active reranker (`OnnxQaReranker`, `OllamaReranker`, or `Top1Reranker`) synthesizes the merged record (prioritizing the `NEWER` memory's updated facts while preserving non-conflicting clauses from `OLDER`, unioning tags case-insensitively, keeping max confidence, re-embedding the survivor, and expiring the `OLDER` record).

---

## ◷ Temporal Filtering (`--as-of` vs `--date`)

- **`--as-of <YYYY-MM-DD|RFC3339>`**: Cumulative point-in-time snapshot — returns memories created on or before that timestamp that were still active at that moment.
- **`--date <YYYY-MM-DD>`**: Single-day filter — returns memories created or updated on that exact UTC calendar day (supported in `lmem recall`, `lmem list`, and `lmem answer`).

---

## ⌖ Run Tests

```bash
# Fast offline test suite (skips ONNX weight download)
LIGHTMEM_QA_DISABLE=1 cargo test --locked

# Full test suite including live ONNX Extractive QA inference
cargo test --locked

# Formatting & strict clippy lint check
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```
