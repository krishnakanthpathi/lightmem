# 🧠 LightMem (`lmem`)

**Ultra-fast, 100% pure-Rust agent memory engine and CLI.**

LightMem combines **SQLite WAL + FTS5 BM25** with **embedded ONNX vector search** (`fastembed`), **temporal point-in-time queries** (`--as-of`), **multi-format JSON/JSONL/OKF ingestion**, and **pure-Rust precision slot extraction** (`--precision`).

---

## 📁 Directory Structure

```text
lightmem/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs          # Core LightMem engine API
│   ├── main.rs         # lmem CLI entrypoint
│   ├── config.rs       # Backend & path configuration
│   ├── embeddings.rs   # ONNX (fastembed), Ollama, and Hash providers
│   ├── exporter.rs     # Open Knowledge Format (OKF) exporter
│   ├── importer.rs     # Universal JSON / JSONL / OKF importer
│   ├── models.rs       # 14 memory categories & records
│   ├── reranker.rs     # Top-1 & Pure-Rust Precision slot extractor
│   ├── search.rs       # Hybrid RRF (BM25 + Cosine Vector) search
│   └── storage.rs      # SQLite WAL + FTS5 persistence layer
├── tests/
│   └── integration_test.rs
└── benches/
    └── bench_harness.rs
```

---

## 🚀 Quick Start

### Build & Install CLI
```bash
cargo install --path . --force
```

### Core CLI Commands
```bash
# Store a memory
lmem remember "PostgreSQL 16 runs on port 5432" -t fact --title "Postgres Port" --tags "db,postgres"

# Hybrid semantic + keyword recall
lmem recall "postgres port"

# Precision question answering & slot extraction
lmem answer "what port does postgres use?" --precision

# Import any JSON / JSONL / OKF file
lmem import memories.json

# Export to Open Knowledge Format (OKF)
lmem export --okf -o backup.okf

# Storage statistics
lmem stats
```

### Run Tests & Benchmarks
```bash
cargo test
cargo bench --bench bench_harness
```
