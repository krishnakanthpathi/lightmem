# Cross-Platform Native SDK Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and package native SDKs for Apple (macOS & iOS via Swift), Android (via Kotlin/JNI), Linux (C/C++ & JVM), and Windows (C/C++ & C# .NET) powered by a unified Rust FFI layer.

**Architecture:** A unified Rust crate (`crates/lightmem-ffi`) exposing Mozilla UniFFI proc-macro bindings (for Swift and Kotlin) alongside an ANSI C-ABI (`c_api.rs`) interface (for C/C++, Linux, Windows, and C# P/Invoke). Platform packaging directories in `bindings/{apple,android,linux,windows}` contain idiomatic package manifests, generated code, headers, and build scripts.

**Tech Stack:** Rust (UniFFI 0.28+, cdylib, staticlib), Swift 6 (SPM, XCFramework), Kotlin (Gradle, JNI, JNA), ANSI C/C++, C# (.NET 8/9 P/Invoke).

**Spec:** `docs/superpowers/specs/2026-10-02-cross-platform-native-sdk-design.md`

## Global Constraints
- Target platform parity: All 14 memory categories (`fact`, `decision`, `instruction`, `preference`, `learning`, `goal`, `commitment`, `artifact`, `event`, `relationship`, `observation`, `error`, `context`, `password`).
- Zero background daemon requirement: All platform SDKs must operate completely in-process using direct Rust SQLite WAL execution.
- Memory safety across language boundaries: All C-allocated strings must provide explicit deallocation primitives (`lmem_free_string`).

## Review Focus
1. Memory leaks in C-ABI string returns: Every string returned across FFI must be freed by callers via `lmem_free_string`.
2. Null/invalid pointer handling in C-ABI: Every function taking `LMemHandle*` must safely check for null pointers.
3. Category string validation: Parsing invalid category names must return structured errors rather than panicking.
4. UniFFI proc-macro compatibility: The FFI crate must compile as both `cdylib` and `staticlib`.
5. Swift Package Manager linking: `Package.swift` must compile cleanly with Swift 6 on macOS.

---

### Task 1: Unified Rust FFI Crate (`crates/lightmem-ffi`)

**Files:**
- Create: `crates/lightmem-ffi/Cargo.toml`
- Create: `crates/lightmem-ffi/src/lib.rs`
- Create: `crates/lightmem-ffi/src/types.rs`
- Create: `crates/lightmem-ffi/src/c_api.rs`
- Modify: `Cargo.toml:1-8`
- Test: `crates/lightmem-ffi/tests/ffi_test.rs`

**Interfaces:**
- Consumes: `lightmem-core::LightMem`, `MemoryRecord`, `MemoryType`, `MemoryStatus`, `ScoredMemory`, `StorageStats`
- Produces: UniFFI exported `LightMem` class and records; ANSI C exports `lmem_open`, `lmem_remember`, `lmem_recall`, `lmem_answer`, `lmem_list`, `lmem_forget`, `lmem_stats`, `lmem_export_okf`, `lmem_import_file`, `lmem_free_string`, `lmem_close`.

- [ ] **Step 1: Write integration test exercising UniFFI objects and C-ABI functions**
- [ ] **Step 2: Add `crates/lightmem-ffi` to root workspace `Cargo.toml`**
- [ ] **Step 3: Implement data structures, UniFFI scaffolding, and ANSI C-ABI in `crates/lightmem-ffi`**
- [ ] **Step 4: Run `cargo test -p lightmem-ffi` to verify 100% test pass**
- [ ] **Step 5: Commit `crates/lightmem-ffi` changes**

---

### Task 2: Apple (macOS & iOS) Swift Package & XCFramework Pipeline

**Files:**
- Create: `bindings/apple/Package.swift`
- Create: `bindings/apple/Sources/LightMem/LightMem.swift`
- Create: `bindings/apple/scripts/build-apple.sh`
- Test: `bindings/apple/Tests/LightMemTests/LightMemTests.swift`

**Interfaces:**
- Consumes: `crates/lightmem-ffi` (staticlib / cdylib)
- Produces: `LightMem` Swift module, `build-apple.sh` building universal `LightMem.xcframework`.

- [ ] **Step 1: Create `bindings/apple/Package.swift` SPM manifest**
- [ ] **Step 2: Implement idiomatic Swift wrapper `LightMem.swift` wrapping the native library**
- [ ] **Step 3: Implement `bindings/apple/scripts/build-apple.sh` for universal XCFramework bundling**
- [ ] **Step 4: Build native staticlib and execute live Swift test on macOS using `swift test` or `swift` driver**
- [ ] **Step 5: Commit Apple platform bindings**

---

### Task 3: Android Kotlin Library & AAR Pipeline

**Files:**
- Create: `bindings/android/build.gradle.kts`
- Create: `bindings/android/settings.gradle.kts`
- Create: `bindings/android/src/main/kotlin/dev/lightmem/LightMem.kt`
- Create: `bindings/android/scripts/build-android.sh`

**Interfaces:**
- Consumes: `crates/lightmem-ffi` (JNI `.so` libraries)
- Produces: `LightMem.kt` Android client with coroutine-friendly methods and AAR packaging script.

- [ ] **Step 1: Create Android Gradle build files (`build.gradle.kts`, `settings.gradle.kts`)**
- [ ] **Step 2: Implement Kotlin client `LightMem.kt` with JNA/JNI native loader**
- [ ] **Step 3: Implement `bindings/android/scripts/build-android.sh` compiling target architectures (`arm64-v8a`, `x86_64`)**
- [ ] **Step 4: Verify Kotlin syntax and build script permissions**
- [ ] **Step 5: Commit Android platform bindings**

---

### Task 4: Linux C/C++ Shared Library & JVM Pipeline

**Files:**
- Create: `bindings/linux/include/lightmem.h`
- Create: `bindings/linux/lightmem.pc.in`
- Create: `bindings/linux/scripts/build-linux.sh`
- Create: `bindings/linux/examples/main.c`

**Interfaces:**
- Consumes: `crates/lightmem-ffi` (`liblightmem.so`)
- Produces: ANSI C header `lightmem.h`, pkg-config template, build script, and working C test example.

- [ ] **Step 1: Write ANSI C header `lightmem.h` with complete docstrings and function declarations**
- [ ] **Step 2: Create `lightmem.pc.in` for system-wide `pkg-config` discovery**
- [ ] **Step 3: Create C sample application `main.c` exercising `lmem_open`, `lmem_remember`, `lmem_recall`, `lmem_close`**
- [ ] **Step 4: Create and verify `build-linux.sh` script**
- [ ] **Step 5: Commit Linux platform bindings**

---

### Task 5: Windows C/C++ DLL & C# .NET SDK Pipeline

**Files:**
- Create: `bindings/windows/include/lightmem.h`
- Create: `bindings/windows/dotnet/LightMem.cs`
- Create: `bindings/windows/dotnet/LightMem.csproj`
- Create: `bindings/windows/scripts/build-windows.ps1`

**Interfaces:**
- Consumes: `crates/lightmem-ffi` (`lightmem.dll`)
- Produces: `LightMem.cs` C# client with `IDisposable`, P/Invoke declarations, JSON deserialization, and PowerShell build script.

- [ ] **Step 1: Create `bindings/windows/include/lightmem.h` header for MSVC/MinGW**
- [ ] **Step 2: Implement C# client `LightMem.cs` (.NET 8.0+) with P/Invoke, safe handles, and automatic DLL resolution**
- [ ] **Step 3: Create `LightMem.csproj` for NuGet / class library packaging**
- [ ] **Step 4: Implement PowerShell automation script `build-windows.ps1`**
- [ ] **Step 5: Commit Windows platform bindings**

---

### Task 6: Documentation, Integration Verification & Final Push

**Files:**
- Modify: `README.md`
- Modify: Obsidian Vault note
- Test: `cargo test --workspace`

- [ ] **Step 1: Update `README.md` with comprehensive documentation and code snippets for Apple (Swift), Android (Kotlin), Linux (C/C++), and Windows (C# .NET)**
- [ ] **Step 2: Update Obsidian Vault project note with completed multi-platform milestones**
- [ ] **Step 3: Run full workspace test suite `cargo test --workspace`**
- [ ] **Step 4: Commit all documentation updates and push to GitHub `main`**
- [ ] **Step 5: Store architectural decision in `memanto`**
