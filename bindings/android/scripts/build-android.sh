#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$ANDROID_DIR/../.." && pwd)"

echo "=== Building LightMem FFI for Android ==="

# Auto-detect ANDROID_NDK if not explicitly set
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    if [ -d "$HOME/Library/Android/sdk/ndk" ]; then
        LATEST_NDK=$(ls -d "$HOME/Library/Android/sdk/ndk/"* 2>/dev/null | sort -V | tail -n 1 || true)
        if [ -n "$LATEST_NDK" ]; then
            export ANDROID_NDK_HOME="$LATEST_NDK"
            echo "✔ Detected Android NDK: $ANDROID_NDK_HOME"
        fi
    fi
fi

# 1. Generate Kotlin Bindings via UniFFI
echo "=== Generating Kotlin Bindings via UniFFI ==="
OUT_KOTLIN="$ANDROID_DIR/src/main/kotlin/dev/lightmem"
mkdir -p "$OUT_KOTLIN"

# Build host library first to extract interface metadata
cd "$REPO_ROOT"
cargo build --release -p lightmem-ffi

LIB_DYLIB="$REPO_ROOT/target/release/liblightmem_ffi.dylib"
if [ ! -f "$LIB_DYLIB" ]; then
    LIB_DYLIB="$REPO_ROOT/target/release/liblightmem_ffi.so"
fi

cargo run -p lightmem-ffi --bin uniffi-bindgen -- generate \
    --library "$LIB_DYLIB" \
    --language kotlin \
    --out-dir "$OUT_KOTLIN"

echo "=== Compiling Native Android JNI Libraries ==="
TARGETS=("aarch64-linux-android:arm64-v8a" "armv7-linux-androideabi:armeabi-v7a" "x86_64-linux-android:x86_64")

for T in "${TARGETS[@]}"; do
    IFS=":" read -r RUST_TARGET JNI_DIR <<< "$T"
    DEST_DIR="$ANDROID_DIR/src/main/jniLibs/$JNI_DIR"
    mkdir -p "$DEST_DIR"

    if command -v cargo-ndk >/dev/null 2>&1 && [ -n "${ANDROID_NDK_HOME:-}" ]; then
        echo "Building JNI shared library for $JNI_DIR ($RUST_TARGET)..."
        cargo ndk -t "$JNI_DIR" build --release -p lightmem-ffi
        cp "$REPO_ROOT/target/$RUST_TARGET/release/liblightmem_ffi.so" "$DEST_DIR/"
    else
        echo "Note: cargo-ndk or ANDROID_NDK_HOME not active in current environment."
        echo "      To build production .so binaries, install: cargo install cargo-ndk"
    fi
done

echo "✔ Android Kotlin bindings generated at: $OUT_KOTLIN"
