# Cross-Platform Native SDK Architecture & Design Specification
**Platforms: Apple (macOS & iOS), Android, Linux, Windows**  
**Date: 2026-10-02**  
**Status: Draft / Pending Approval**

---

## 1. Executive Summary & Goals

LightMem is a high-performance agent memory engine built in Rust. It combines SQLite WAL, FTS5 BM25 keyword search, in-process ONNX vector embeddings, and Reciprocal Rank Fusion (RRF) with point-in-time (`--as-of`) temporal queries.

While Python (`lightmem-py`) and CLI (`lmem`) are production-ready, native mobile, desktop, and server developers require first-class native SDKs with **zero Python or CLI subprocess overhead**:
1. **Apple (macOS & iOS):** Swift Package (`LightMem`) with SwiftUI/AppKit/UIKit support, distributed via Swift Package Manager (SPM) and universal `LightMem.xcframework`.
2. **Android:** Kotlin library (`LightMemAndroid`) packaged as an Android Archive (`.aar`) with JNI `.so` binaries for ARM64 and x86_64.
3. **Linux:** C/C++ native shared library (`liblightmem.so`), ANSI C header (`lightmem.h`), `pkg-config` specification (`lightmem.pc`), and Kotlin/JVM JAR.
4. **Windows:** Dynamic Link Library (`lightmem.dll` + `lightmem.lib`), ANSI C header, and a modern C# .NET wrapper (`LightMem.NET`) for WPF, WinUI 3, MAUI, and ASP.NET.

---

## 2. Multi-Platform Support Matrix

| Platform | Primary Language | Packaging / Distribution | Native Artifacts |
| :--- | :--- | :--- | :--- |
| **Apple macOS** | Swift (Swift 5.9+) | Swift Package (`Package.swift`) | `LightMem.xcframework` (macOS arm64/x86_64) |
| **Apple iOS** | Swift / SwiftUI | Swift Package (`Package.swift`) | `LightMem.xcframework` (iOS Device arm64 + Sim arm64/x86_64) |
| **Android** | Kotlin / Java | Gradle Android Library (`.aar`) | `jniLibs/{arm64-v8a, armeabi-v7a, x86_64}/liblightmem_ffi.so` |
| **Linux (Native)** | C / C++ / Zig / Rust | System Shared Library & pkg-config | `liblightmem.so`, `include/lightmem.h`, `lightmem.pc` |
| **Linux (JVM)** | Kotlin / Java | Gradle JVM JAR | `lightmem-jvm.jar` with bundled Linux `.so` via JNA |
| **Windows (Native)**| C / C++ / Win32 | Dynamic Link Library & Headers | `lightmem.dll`, `lightmem.lib`, `include/lightmem.h` |
| **Windows (.NET)**  | C# (.NET 8.0+) | NuGet Package / Single-file C# SDK | `LightMem.cs` with P/Invoke bridge and auto-DLL loading |

---

## 3. Core Architecture: `crates/lightmem-ffi`

To avoid duplicating glue code across 5 operating systems, we introduce a single unified FFI bridge crate: **`crates/lightmem-ffi`**.

```
/Users/krishnakanth/Projects/lightmem/
├── crates/
│   ├── lightmem-core/         # Core Rust memory engine
│   ├── lightmem-cli/          # CLI binary (`lmem`)
│   ├── lightmem-py/           # PyO3 Python C-extension
│   └── lightmem-ffi/          # NEW: UniFFI definitions + Standard C-ABI exports
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs         # UniFFI interface exports (Swift, Kotlin)
│       │   ├── types.rs       # FFI record conversions & 14 categories
│       │   └── c_api.rs       # Standard C-ABI functions for C/C++/C#/Linux/Windows
│       └── uniffi.toml        # UniFFI configuration
```

### 3.1 Two-Pronged FFI Strategy
1. **Mozilla UniFFI (Proc-Macro standard):** Generates memory-safe, idiomatic Swift and Kotlin bindings directly from Rust structs and methods using `#[uniffi::export]`. Handles all string conversions, error conversions, and memory cleanup automatically.
2. **ANSI C-ABI Layer (`c_api.rs`):** Exposes simple `extern "C"` functions returning C strings and pointers with strict lifecycle methods (`lmem_free_string`, `lmem_free`). Used by Linux C/C++, Windows C/C++, and C# .NET P/Invoke callers.

---

## 4. Universal Type & API Parity

All platforms expose identical models and operations matching the core engine:

### 4.1 Data Structures
- **`MemoryType` (14 Categories):** `Fact`, `Decision`, `Instruction`, `Preference`, `Learning`, `Goal`, `Commitment`, `Artifact`, `Event`, `Relationship`, `Observation`, `Error`, `Context`, `Password`.
- **`MemoryStatus`:** `Active`, `Expired`, `Archived`.
- **`MemoryRecord`:** `id`, `category`, `title`, `content`, `tags`, `confidence`, `status`, `provenance`, `created_at`, `updated_at`, `expired_at`.
- **`ScoredMemory`:** `memory`, `score`.
- **`AnswerResult`:** `answer`, `selected_memory`, `confidence`, `reranker_used`.
- **`StorageStats`:** `total_memories`, `active_memories`, `expired_memories`, `total_vectors`, `by_category` (key-value counts).

### 4.2 Methods on `LightMem`
1. `new(db_path: Option<String>, global_db: bool) -> Result<LightMem>`
2. `remember(content: String, category: Option<String>, title: Option<String>, tags: Vec<String>, confidence: Option<f32>) -> Result<MemoryRecord>`
3. `recall(query: String, category: Option<String>, as_of: Option<String>, limit: Option<u32>, min_similarity: Option<f32>) -> Result<Vec<ScoredMemory>>`
4. `answer(question: String, needle: Option<bool>, category: Option<String>, as_of: Option<String>, limit: Option<u32>) -> Result<AnswerResult>`
5. `list(category: Option<String>, status: Option<String>, as_of: Option<String>, limit: Option<u32>) -> Result<Vec<MemoryRecord>>`
6. `forget(id: String, hard: bool) -> Result<bool>`
7. `stats() -> Result<StorageStats>`
8. `export_okf(output_path: Option<String>) -> Result<String>`
9. `import_file(path: String) -> Result<u64>`

---

## 5. Platform Bindings Specification

### 5.1 Apple (macOS & iOS) — Swift Package
- **Directory:** `bindings/apple/`
- **Manifest:** `Package.swift`
- **Output:** `LightMem.xcframework` wrapping:
  - macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`)
  - iOS Device (`aarch64-apple-ios`)
  - iOS Simulator (`aarch64-apple-ios-sim`, `x86_64-apple-ios-sim`)
- **Swift Usage Example:**
  ```swift
  import LightMem

  let client = try LightMem(dbPath: nil, globalDb: false)
  let record = try client.remember(
      content: "CoreData is replaced with LightMem SQLite WAL",
      category: "decision",
      title: "Storage Engine",
      tags: ["apple", "storage"],
      confidence: 1.0
  )
  let results = try client.recall(query: "storage engine", category: "decision", limit: 5)
  for r in results {
      print("[\(r.memory.category)] \(r.memory.title): score \(r.score)")
  }
  ```

### 5.2 Android — Kotlin & AAR
- **Directory:** `bindings/android/`
- **Build System:** Gradle (`build.gradle.kts`)
- **Native ABI Targets:** `arm64-v8a`, `armeabi-v7a`, `x86_64`
- **Kotlin Usage Example:**
  ```kotlin
  import dev.lightmem.LightMem

  val client = LightMem(dbPath = context.filesDir.resolve("memories.db").absolutePath)
  val record = client.remember(
      content = "User prefers dark mode on OLED displays",
      category = "preference",
      title = "Theme Preference",
      tags = listOf("ui", "display"),
      confidence = 0.95f
  )
  val answer = client.answer("what theme does the user want?", needle = true)
  println("Answer: ${answer.answer}")
  ```

### 5.3 Linux — C/C++ Shared Library & pkg-config
- **Directory:** `bindings/linux/`
- **Artifacts:** `liblightmem.so`, `include/lightmem.h`, `lightmem.pc`
- **C Usage Example:**
  ```c
  #include <stdio.h>
  #include "lightmem.h"

  int main() {
      LMemHandle* lm = lmem_open(NULL, 0);
      char* json_out = lmem_remember(lm, "PostgreSQL configured", "fact", "DB Setup", "postgres", 0.95f);
      printf("Stored: %s\n", json_out);
      lmem_free_string(json_out);
      lmem_close(lm);
      return 0;
  }
  ```

### 5.4 Windows — C/C++ DLL & C# .NET Client
- **Directory:** `bindings/windows/`
- **Artifacts:** `lightmem.dll`, `lightmem.lib`, `dotnet/LightMem.cs`
- **C# .NET Usage Example:**
  ```csharp
  using LightMem;

  using var lm = new LightMemClient();
  var record = lm.Remember("Windows 11 Mica material enabled", category: "decision", title: "UI Polish");
  Console.WriteLine($"Stored: {record.Id} - {record.Title}");

  var results = lm.Recall("Mica material", limit: 3);
  foreach (var r in results) {
      Console.WriteLine($"Found {r.Memory.Title} with score {r.Score}");
  }
  ```

---

## 6. Build Automation & Packaging Scripts

Each platform receives an automated, robust build script:
1. `bindings/apple/scripts/build-apple.sh`: Generates Swift bindings via `uniffi-bindgen`, compiles Rust targets, and builds universal `LightMem.xcframework`.
2. `bindings/android/scripts/build-android.sh`: Generates Kotlin bindings, compiles target `.so` binaries via `cargo-ndk`, and builds `lightmem.aar`.
3. `bindings/linux/scripts/build-linux.sh`: Compiles `liblightmem.so`, generates `lightmem.h`, and formats `lightmem.pc`.
4. `bindings/windows/scripts/build-windows.ps1` & cross-compilation target: Builds `lightmem.dll`, generates headers and C# source bindings.

---

## 7. Verification & Testing Strategy
- **Rust FFI Unit Tests:** `cargo test -p lightmem-ffi` verifying both UniFFI bindings and C-ABI exports.
- **macOS Swift Verification:** Run a live Swift test script on host macOS using `swift-driver 1.168.6` against the compiled static/dynamic library.
- **C-ABI Verification:** Compile and run a minimal C test executable linking against `liblightmem_ffi`.
- **Build Scripts Validation:** Verify that build scripts generate all required headers, manifests, and scaffolding.
