# 🧠 LightMem

> Ultra-fast, lightweight, cross-platform agent memory engine written in Rust.  
> Zero Docker. Zero background daemons. Single SQLite database with FTS5 BM25 + Vector Hybrid search.

---

## 🚀 Key Highlights

* **Instant Startup:** Sub-5ms CLI execution. No background servers or network latency.
* **Hybrid Search (BM25 + Vectors):** Combines SQLite FTS5 for exact keyword/symbol precision with Vector Cosine Similarity and Reciprocal Rank Fusion (RRF).
* **Cross-Platform Ready:** Core engine compiles to native libraries for Android (Kotlin via UniFFI) and macOS/iOS (Swift via UniFFI) without Termux.
* **Dual-Tier Resolution:** Automatically stores and recalls from local project repository (`./.lightmem.db`) or global user store (`~/.lightmem/memories.db`).
* **Temporal Point-in-Time Queries:** Query what was active on any historical date with `--as-of YYYY-MM-DD`.
* **Standardized Ingestion & Export:** Native support for Open Knowledge Format (`--okf`) and JSON arrays from Memanto/Mem0/Letta.

---

## 🛠️ CLI Quickstart (`lmem`)

### 1. Store Memories
```bash
# Store an architectural decision in the local project
lmem remember "Use Next.js App Router exclusively; Pages Router is deprecated" \
  --type decision \
  --tags "nextjs,react,routing" \
  --confidence 1.0

# Store a global personal preference across all projects (-g)
lmem remember "Always format shell commands in copyable fenced code blocks" \
  --type preference \
  --tags "formatting,cli,shell" \
  --global
```

### 2. Recall Memories (Hybrid Search)
```bash
# Natural language search (hybrid BM25 + vector similarity)
lmem recall "what database engine do we use"

# Filter by category
lmem recall "auth configuration" --type decision

# Output as raw JSON for LLM agent prompts
lmem recall "styling rules" --json
```

### 3. Precision Answering & Reranking (`lmem answer`)
Ask a direct natural language question. By default, it returns the top-ranked candidate instantly (< 5ms). Toggle `--needle` to invoke **Needle 3** (an ultra-fast 35MB Apple Silicon SLM) for candidate disambiguation and factual slot extraction.

```bash
# Instant Rank-1 Answer (0ms latency, zero python invocation)
lmem answer "what port does redis run on?"

# Needle 3 Precision Disambiguation & Slot Extraction
lmem answer "what port does redis run on?" --needle

# Filter by category and output structured JSON
lmem answer "what database engine do we use?" --needle --type fact --json

# Set Needle 3 as default reranker
lmem config --reranker needle   # Switch default to Needle 3
lmem config --reranker top1     # Switch default to instant Top-1
```

### 4. Point-in-Time Historical Queries (`--as-of`)
```bash
# Reconstruct what memories were active on August 5th, 2026
lmem recall "infrastructure setup" --as-of 2026-08-05
```

### 5. Open Knowledge Format Export & Import
```bash
# Export all active memories to a clean OKF Markdown bundle
lmem export --okf -o ./PROJECT_MEMORY.md

# Import external memories (.json or .okf / .md)
lmem import ./PROJECT_MEMORY.md
lmem import ./memanto_export.json
```

### 6. Retiring & Forgetting Memories
```bash
# Soft-retire a memory (preserves historical audit trail for --as-of queries)
lmem forget <MEMORY_ID>

# Permanently delete
lmem forget <MEMORY_ID> --hard
```

### 7. Storage Statistics & Configuration
```bash
# View active database and category breakdown
lmem stats

# 1. Local in-process ONNX (Default, ~70MB, zero daemons, CPU/Metal accelerated)
lmem config --backend onnx --onnx-model bge-small     # BGE Small v1.5 (384d)
lmem config --backend onnx --onnx-model minilm        # All-MiniLM-L6-v2 (384d)
lmem config --backend onnx --onnx-model nomic         # Nomic Embed v1.5 (768d)

# 2. Custom local ONNX model (directory containing model.onnx and tokenizer.json)
lmem config --backend onnx --onnx-model /path/to/custom_model_dir/

# 3. Remote / Local Ollama server
lmem config --backend ollama --url "http://100.75.149.115:7777" --model "nomic-embed-text"

# 4. Instant hash fallback (offline/testing)
lmem config --backend hash
```

---

## 📦 Project Architecture

```
/Users/krishnakanth/Projects/lightmem/
├── Cargo.toml
├── crates/
│   ├── lightmem-core/         # Core Rust library (SQLite, FTS5, Vectors, RRF, Importers)
│   │   ├── scripts/
│   │   │   └── needle_picker.py # Needle 3 Action SLM disambiguator & slot extractor
│   │   ├── src/
│   │   │   ├── config.rs      # Global/local paths & backend config
│   │   │   ├── embeddings.rs  # Local ONNX + Ollama + Hash fallback providers
│   │   │   ├── exporter.rs    # OKF bundle generation
│   │   │   ├── importer.rs    # Integratable JSON & OKF parsing pipeline
│   │   │   ├── models.rs      # 13 MemoryTypes, MemoryRecord, ScoredMemory
│   │   │   ├── reranker.rs    # Top1 & Needle 3 precision reranker engines
│   │   │   ├── search.rs      # Hybrid search & Reciprocal Rank Fusion (RRF)
│   │   │   └── storage.rs     # SQLite WAL mode, FTS5 sync triggers, BLOB vectors
│   └── lightmem-cli/          # Fast CLI application (`lmem`)
│       └── src/main.rs        # CLI subcommands: remember, recall, answer, list, forget, config, stats
└── wrappers/                  # Native bindings (Android UniFFI Kotlin, macOS/iOS Swift)
```

---

## 📱 Future Native Mobile & Desktop Wrappers

Because `lightmem-core` is written in standard Rust with zero dynamic interpreter dependencies, it compiles directly into:
1. **Android (`.aar`):** Native Kotlin bindings using UniFFI for Jetpack Compose apps.
2. **Apple (`.xcframework`):** Native Swift bindings using UniFFI for SwiftUI and macOS menu bar apps.
