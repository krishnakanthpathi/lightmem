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

# Store a secure token or credential
lmem remember "ghp_xxxxxxxxxxxxxxxxxxxx" \
  --type password \
  --title "GitHub Deployment Token" \
  --tags "auth,token,github"

# Store a global personal preference across all projects (-g)
lmem remember "Always format shell commands in copyable fenced code blocks" \
  --type preference \
  --tags "formatting,cli,shell" \
  --global
```

> **Supported Categories (14):** `fact`, `decision`, `instruction`, `preference`, `learning`, `goal`, `commitment`, `artifact`, `event`, `relationship`, `observation`, `error`, `context`, `password`.


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
│   │   │   ├── models.rs      # 14 MemoryTypes, MemoryRecord, ScoredMemory
│   │   │   ├── reranker.rs    # Top1 & Needle 3 precision reranker engines
│   │   │   ├── search.rs      # Hybrid search & Reciprocal Rank Fusion (RRF)
│   │   │   └── storage.rs     # SQLite WAL mode, FTS5 sync triggers, BLOB vectors
│   ├── lightmem-cli/          # Fast CLI application (`lmem`)
│   │   └── src/main.rs        # CLI subcommands: remember, recall, answer, list, forget, config, stats
│   ├── lightmem-py/           # Native PyO3 C-Extension (direct `import lightmem` in Python)
│   │   ├── Cargo.toml
│   │   ├── pyproject.toml     # Maturin packaging configuration
│   │   └── src/lib.rs         # PyO3 bindings & class definitions
│   └── lightmem-ffi/          # Universal FFI Layer (UniFFI + ANSI C-ABI exports)
│       ├── src/lib.rs         # Mozilla UniFFI proc-macro object & exports
│       ├── src/types.rs       # 14 Memory categories & record representations
│       └── src/c_api.rs       # ANSI C function exports for Linux/Windows/C#
├── bindings/
│   ├── apple/                 # Swift Package (macOS & iOS SPM / XCFramework)
│   │   ├── Package.swift      # Swift Package Manager manifest
│   │   └── Sources/LightMem/  # Swift wrapper & type definitions
│   ├── android/               # Android / Kotlin Gradle library
│   │   ├── build.gradle.kts   # AAR build configuration
│   │   └── src/main/kotlin/   # Kotlin client & UniFFI bindings
│   ├── linux/                 # Linux C/C++ native shared library & pkg-config
│   │   ├── include/lightmem.h # ANSI C API header
│   │   └── lightmem.pc.in     # pkg-config specification
│   └── windows/               # Windows native DLL & C# .NET SDK
│       ├── include/lightmem.h # Windows C/C++ header with dllexport/dllimport
│       └── dotnet/            # C# .NET 8/9 class library & P/Invoke wrapper
```

---

## 🐍 Native Python Usage (`import lightmem`)

LightMem is compiled directly into a native CPython C-extension using PyO3. Zero subprocesses, zero CLI overhead—pure in-memory Rust speed.

### Installation
```bash
# Build & install wheel directly using maturin
cd crates/lightmem-py
maturin build --release
pip install ../../target/wheels/*.whl
```

### Direct Python Example
```python
import lightmem

# Initialize in-memory native Rust engine
lm = lightmem.LightMem()  # Uses .lightmem.db or pass db_path="..."

# 1. Store memory
mem = lm.remember(
    content="PostgreSQL 16 runs on port 5432 with replication enabled",
    category="decision",
    title="PostgreSQL Setup",
    tags=["db", "infra"],
    confidence=0.95
)
print("Stored ID:", mem.id)

# 2. Hybrid Recall (BM25 + BGE-Small Vector RRF)
results = lm.recall("what port does postgres use?", limit=5)
for r in results:
    print(f"[{r.memory.category}] {r.memory.title} (score: {r.score:.3f})")
    print(f"  {r.memory.content}")

# 3. Answer synthesis (Fast Top-1 vs Precision Needle 3)
ans_fast = lm.answer("what port is postgres on?")
print("Answer (Fast):", ans_fast.answer)

ans_needle = lm.answer("what port is postgres on?", needle=True)
print("Answer (Needle 3):", ans_needle.answer)

# 4. Storage statistics
stats = lm.stats()
print("Total memories:", stats.total_memories, "By category:", stats.by_category)

# 5. Forget memory
lm.forget(mem.id, hard=False)
```

---

## 🌍 Cross-Platform Native SDKs

LightMem provides first-class native developer libraries powered by a unified Rust FFI layer (`crates/lightmem-ffi`), delivering pure in-process SQLite WAL speed with zero Python or CLI subprocess overhead.

### 🍏 1. Apple (macOS & iOS) — Swift Package (`import LightMem`)
Integrated directly via Swift Package Manager (`Package.swift`) or universal `LightMem.xcframework`.

```bash
# Build Swift bindings & package
./bindings/apple/scripts/build-apple.sh
```

```swift
import LightMem

// Open local project store (or pass dbPath / global: true)
let client = try LightMem()

// 1. Remember fact, preference, or password
let mem = try client.remember(
    "PostgreSQL 16 runs on port 5432 with replication enabled",
    category: "decision",
    title: "Postgres Setup",
    tags: ["database", "postgres"]
)
print("Stored ID:", mem.id)

// 2. Hybrid Recall (BM25 + Vector Cosine RRF)
let results = try client.recall("what port does postgres use?", limit: 5)
for r in results {
    print("[\(r.memory.category)] \(r.memory.title) (score: \(r.score))")
}

// 3. Factual Answer Synthesis
let ans = try client.answer("what port does postgres use?", needle: true)
print("Answer:", ans.answer)
```

---

### 🤖 2. Android — Kotlin Library (`dev.lightmem.LightMem`)
Packaged as an Android Archive (`.aar`) with JNI `.so` binaries for `arm64-v8a`, `armeabi-v7a`, and `x86_64`.

```bash
# Generate Kotlin bindings and compile JNI libraries
./bindings/android/scripts/build-android.sh
```

```kotlin
import dev.lightmem.LightMem

// Initialize native engine in your Application or ViewModel
val client = LightMem(dbPath = context.filesDir.resolve("memories.db").absolutePath)

// Remember memory
val record = client.remember(
    content = "User prefers system-dark OLED theme",
    category = "preference",
    title = "UI Theme",
    tags = listOf("theme", "display")
)

// Recall memories
val results = client.recall(query = "theme preference", limit = 5u)
for (r in results) {
    println("[${r.memory.category}] ${r.memory.title}")
}
```

---

### 🐧 3. Linux — C / C++ Native Shared Library (`#include "lightmem.h"`)
Ships with standard `liblightmem.so`, ANSI C99/C++ header `lightmem.h`, and `pkg-config` specification (`lightmem.pc`).

```bash
# Build Linux shared library & pkg-config
./bindings/linux/scripts/build-linux.sh
```

```c
#include <stdio.h>
#include "lightmem.h"

int main() {
    LMemHandle* lm = lmem_open("/path/to/memories.db", 0);
    
    char* rec_json = lmem_remember(
        lm, 
        "Linux kernel uses eBPF for programmable telemetry", 
        "fact", 
        "eBPF Architecture", 
        "linux,ebpf", 
        0.98f
    );
    printf("Stored: %s\n", rec_json);
    lmem_free_string(rec_json);

    char* search_json = lmem_recall(lm, "eBPF telemetry", "fact", NULL, 5, -1.0f);
    printf("Recalled: %s\n", search_json);
    lmem_free_string(search_json);

    lmem_close(lm);
    return 0;
}
```

---

### 🪟 4. Windows — C# .NET 8/9 SDK (`using LightMem;`) & Win32 DLL
Provides `lightmem.dll` alongside a single-file C# SDK (`LightMem.cs`) supporting WPF, WinUI 3, MAUI, and ASP.NET Core with automatic P/Invoke marshalling.

```powershell
# Build Windows DLL and .NET SDK library
powershell ./bindings/windows/scripts/build-windows.ps1
```

```csharp
using LightMem;

using var lm = new LightMemClient();

// Store memory
var record = lm.Remember(
    "Windows 11 Mica material enabled for modern fluent shell",
    category: "decision",
    title: "Fluent Shell Styling",
    tags: new[] { "winui", "fluent", "desktop" }
);

// Hybrid recall
var results = lm.Recall("fluent shell styling", limit: 5);
foreach (var r in results)
{
    Console.WriteLine($"[{r.Memory.Category}] {r.Memory.Title} (score: {r.Score:F3})");
}

// Direct question answering with Needle 3
var answer = lm.Answer("what material is used for the shell?", needle: true);
Console.WriteLine($"Answer: {answer.Answer}");
```

