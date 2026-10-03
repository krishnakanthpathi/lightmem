# ❖ LightMem (`lmem`)

**Ultra-fast, 100% pure-Rust local agent memory engine and CLI.**

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **Native Needle 3 C-FFI reranking & structured extraction** (`libneedle`), **temporal point-in-time queries** (`--as-of`), **14-category auto-inference**, and **multi-format JSON / JSONL / OKF / Memanto Markdown ingestion**.

---

## ◈ One-Line Install (macOS & Linux)

Install `lmem` + Native Needle 3 runtime (`libneedle` + `needle3.cact`) with a single command:

```bash
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh
```

### Build from Source (Rust / Cargo)
```bash
git clone https://github.com/krishnakanthpathi/lightmem.git
cd lightmem
cargo install --path . --force
```

---

## ◫ Architecture (MVC / MVP)

```text
lightmem/
├── Cargo.toml
├── install.sh                      # Universal macOS / Linux installer
├── src/
│   ├── lib.rs                      # Library root & public re-exports
│   ├── main.rs                     # CLI router (Clap -> Controller -> View)
│   ├── models/                     # Domain Entities & Value Objects
│   │   ├── mod.rs
│   │   ├── memory.rs               # 14-category MemoryType::infer, MemoryRecord, PaginatedResult
│   │   ├── config.rs               # LightMemConfig & RerankerProvider
│   │   └── stats.rs                # StorageStats telemetry model
│   ├── repositories/               # Data Access Layer
│   │   ├── mod.rs
│   │   └── sqlite_repo.rs          # SQLite WAL + FTS5 + vector blob persistence
│   ├── services/                   # Business & Domain Engines
│   │   ├── mod.rs
│   │   ├── embeddings.rs           # FastEmbed ONNX (AllMiniLML6V2), Ollama, & Hash engines
│   │   ├── search.rs               # Hybrid RRF (BM25 + Cosine) + Temporal Decay + MMR
│   │   ├── reranker.rs             # Top-1 & Native Needle 3 C-FFI worker thread
│   │   ├── importer.rs             # JSON / JSONL / OKF / Memanto Markdown importer
│   │   └── exporter.rs             # Open Knowledge Format (OKF) bundle exporter
│   ├── controllers/                # Application Orchestration
│   │   ├── mod.rs
│   │   └── memory_controller.rs    # MemoryController (LightMem) with OnceLock lazy ONNX init
│   └── views/                      # Presentation Layer
│       ├── mod.rs
│       └── cli_view.rs             # 24-bit TrueColor Crimson Cloud HUD, vector icons, & JSON output
└── tests/
    └── integration_test.rs
```

---

## ✦ Core CLI Usage

```bash
# Display Crimson Cloud HUD & live vault telemetry
lmem

# Store a memory (category is auto-inferred across 14 types if -t is omitted)
lmem remember "PostgreSQL 16 runs on port 5432" --title "Postgres Port" --tags "db,postgres"

# Hybrid semantic + keyword recall
lmem recall "postgres port" --limit 5

# Factual question answering (uses Native Needle 3 C-FFI by default; override with -r top1 or -r needle)
lmem answer "what port does postgres use?"
lmem answer "what port does postgres use?" -r top1

# Paginated chronological list
lmem list --page 1 --limit 20
lmem list --offset 20 --limit 20 -t decision

# Import structured or unstructured JSON, JSONL, OKF directory, or Memanto memory.md
lmem import memories.json
lmem import ~/.memanto/on-prem/exports/memory.md

# Export to Open Knowledge Format (OKF)
lmem export --okf -o backup.okf

# Vault storage statistics & category distribution
lmem stats
```

---

## ⌖ Run Tests

```bash
cargo test
```
