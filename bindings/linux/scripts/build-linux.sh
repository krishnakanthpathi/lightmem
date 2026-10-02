#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LINUX_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$LINUX_DIR/../.." && pwd)"

echo "=== Building LightMem for Linux (C/C++ Shared Library & Headers) ==="

cd "$REPO_ROOT"
cargo build --release -p lightmem-ffi

mkdir -p "$LINUX_DIR/lib"
if [ -f "$REPO_ROOT/target/release/liblightmem_ffi.so" ]; then
    cp "$REPO_ROOT/target/release/liblightmem_ffi.so" "$LINUX_DIR/lib/liblightmem.so"
elif [ -f "$REPO_ROOT/target/release/liblightmem_ffi.dylib" ]; then
    cp "$REPO_ROOT/target/release/liblightmem_ffi.dylib" "$LINUX_DIR/lib/liblightmem.dylib"
fi

# Generate pkg-config file
PREFIX="${PREFIX:-/usr/local}"
sed -e "s|@PREFIX@|$PREFIX|g" "$LINUX_DIR/lightmem.pc.in" > "$LINUX_DIR/lightmem.pc"

echo "✔ Linux artifacts prepared in $LINUX_DIR"
