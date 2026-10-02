#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$APPLE_DIR/../.." && pwd)"

echo "=== Building LightMem FFI for Apple Platforms ==="

# 1. Compile release dylib/staticlib for host macOS
cd "$REPO_ROOT"
cargo build --release -p lightmem-ffi

LIB_DYLIB="$REPO_ROOT/target/release/liblightmem_ffi.dylib"
if [ ! -f "$LIB_DYLIB" ]; then
    echo "Error: Native library not found at $LIB_DYLIB"
    exit 1
fi

echo "=== Generating Swift Bindings via UniFFI ==="
OUT_SWIFT="$APPLE_DIR/Sources/LightMem"
OUT_FFI="$APPLE_DIR/Sources/LightMemFFI/include"

mkdir -p "$OUT_SWIFT" "$OUT_FFI"

cargo run -p lightmem-ffi --bin uniffi-bindgen -- generate \
    --library "$LIB_DYLIB" \
    --language swift \
    --out-dir "$OUT_SWIFT"

# Move C-header and modulemap to LightMemFFI
if [ -f "$OUT_SWIFT/lightmem_ffiFFI.h" ]; then
    mv "$OUT_SWIFT/lightmem_ffiFFI.h" "$OUT_FFI/lightmem_ffiFFI.h"
fi
if [ -f "$OUT_SWIFT/lightmem_ffiFFI.modulemap" ]; then
    mv "$OUT_SWIFT/lightmem_ffiFFI.modulemap" "$OUT_FFI/module.modulemap"
fi

echo "=== Packaging Apple Universal Frameworks ==="
mkdir -p "$APPLE_DIR/Frameworks"
cp "$REPO_ROOT/target/release/liblightmem_ffi.a" "$APPLE_DIR/Frameworks/liblightmem_ffi.a"
cp "$REPO_ROOT/target/release/liblightmem_ffi.dylib" "$APPLE_DIR/Frameworks/liblightmem_ffi.dylib"

echo "✔ Apple Swift Package successfully built and generated at: $APPLE_DIR"
