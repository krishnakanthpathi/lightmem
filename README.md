# ❖ LightMem (`lmem`)

**Local agent memory engine and CLI, written in Rust with SQLite, ONNX Runtime, and optional native Needle inference.**

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **entity-aware candidate selection and Native Needle 3 C-FFI structured extraction** (`libneedle`), **temporal point-in-time queries** (`--as-of`), **14-category auto-inference**, and **multi-format JSON / JSONL / OKF / Memanto Markdown ingestion**.

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
│   │   ├── embeddings.rs           # FastEmbed ONNX (BGE by default), Ollama, & Hash engines
│   │   ├── search.rs               # Hybrid RRF (BM25 + Cosine) + lexical boosts
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

## Safe embedding model changes

```bash
# Shows the old/new model and selected database, then asks [y/N].
lmem config --backend onnx --onnx-model minilm

# Explicit approval for scripts (same model-change command).
lmem config --backend onnx --onnx-model minilm --yes

# Rebuild the current model's index, including legacy databases with unknown vectors.
lmem reindex
lmem --db /path/to/project.db reindex --yes
```

Progress is printed to stderr in batches. The old index remains usable until all
replacement vectors validate and commit together. A failed model load, invalid
vector, or concurrent memory/index change leaves the previous index intact.
Declining a configuration-change prompt leaves both configuration and embeddings
unchanged. Noninteractive and JSON commands never prompt or start migration
without explicit approval. `reindex` repairs missing vectors as well.

Configuration is shared, but each database has its own embedding identity and
requires its own migration. Only the selected database is migrated. Listing,
exporting, and deleting memories do not need to load an embedding model.
Existing databases have unknown model identities; they must be explicitly
reindexed before semantic reads/writes, even if the configured model appears
unchanged. Numbered SQLite schema upgrades repair old FTS entries automatically.

Model identity includes backend/model, preprocessing version, and engine version;
custom ONNX directories are fingerprinted by file contents. Ollama identity uses
server URL and model name. If the weights behind an unchanged Ollama tag or hosted
ONNX model change, explicitly run `reindex`; mutable remote revisions are not
automatically detected. Migration stages vectors in memory, so peak RAM grows
with vault size. `LIGHTMEM_CONFIG_DIR` isolates configuration/model cache for tests.

## Deduplication

Deduplication is deterministic and does **not** use Needle. Active memories merge
only when category and content match after trimming outer whitespace. Case,
internal whitespace, code indentation, secrets, different categories, and
paraphrases remain distinct. Ordinary duplicate writes preserve the existing ID,
union tags case-insensitively, retain the earliest creation time, keep the highest
confidence and prefer explicit titles. The returned record is the stored result.
The final merged card is re-embedded in the same transaction.

`lmem dedup` applies the same rule to existing records, keeping the oldest ID
(with ID as the tie-break), refreshing its embedding and removing redundant
records. It is idempotent. Explicit same-ID imports update that record; backups
preserve records as-is and do not deduplicate them. No semantic similarity model
is allowed to automatically delete merely similar memories.

## Backups and strict imports

```bash
lmem export --json -o backup.json   # all statuses and complete record metadata
lmem import backup.json            # restore IDs/timestamps/status/provenance/tags
lmem export --okf -o readable.md   # active records; lossless OKF v2 metadata
lmem import readable.md
lmem import external.json --enrich # explicitly enable optional Needle enrichment
```

JSON backups use `lightmem-backup-v1`. OKF v2 includes a canonical JSON record in
each HTML comment; readable text is for display and the metadata is authoritative
on import. Headings, separators, code fences, commas in tags and Unicode survive
round trips. Legacy OKF v1 remains readable, but its unescaped Markdown boundaries
cannot recover content already lost in an older export/import.

Imports validate all input before committing. Malformed JSONL reports the line
number; unsupported records fail rather than being silently skipped. Directory
imports use one transaction and skip symlinks. Parsing does not call Needle unless
`--enrich` is given. Embedding failures roll back writes rather than silently
creating unindexed records. Bulk embedding uses batches; the write transaction
holds a database writer lock during inference, so long imports can block writers.

## Answer reliability and validation

Retrieval keeps the RRF score through final ranking. `score` is a ranking score,
not a probability; `--min-similarity` filters by raw cosine similarity. Candidate
hydration is batched and vector selection bounds the sorted candidate pool,
although exact vector search still scans the vault.

Needle extracts only the requested field, and only accepted, grounded calls are
used. Suppressed/ungrounded calls are rejected. Native confidence is never
artificially raised; missing confidence is reported as zero. The result reports
`needle-3`, `regex-fallback`, or `none`; regex confidence is zero (uncalibrated).
Missing entity matches, ambiguous regex matches, and rejected native output produce an abstention. Conservative entity matching
can miss paraphrases. `top1` returns the first retrieved record after an evidence
check. Neither mode's confidence should be treated as a calibrated probability.

```bash
LIGHTMEM_NEEDLE_DISABLE=1 cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
# Optional synthetic smoke evaluation with native Needle assets installed:
cargo run --locked --example needle_eval
```

Pull requests run tests, formatting and lint checks on Linux and macOS. Native
Needle inference is an optional evaluation because the model/library are not
bundled into CI. The eight-case example includes grounded facts and absent facts;
it is a smoke check, not a comprehensive accuracy benchmark.
