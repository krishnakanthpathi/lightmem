#!/usr/bin/env sh
# LightMem Universal Single-Command Installer (macOS & Linux)
# Usage: curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

set -e

REPO="krishnakanthpathi/lightmem"
INSTALL_DIR="${LIGHTMEM_INSTALL_DIR:-$HOME/.local/bin}"
NEEDLE_CACHE_DIR="$HOME/.cache/cactus-needle/v3/3.1.0"

printf "\033[1;36m🧠 Installing LightMem (lmem)...\033[0m\n"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        OS_TARGET="apple-darwin"
        NEEDLE_LIB="libneedle.dylib"
        NEEDLE_WHL_PATTERN="macosx.*arm64\.whl"
        ;;
    Linux)
        OS_TARGET="unknown-linux-gnu"
        NEEDLE_LIB="libneedle.so"
        if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
            NEEDLE_WHL_PATTERN="manylinux.*aarch64\.whl"
        else
            NEEDLE_WHL_PATTERN="manylinux.*x86_64\.whl"
        fi
        ;;
    *)
        printf "\033[1;31m✖ Unsupported OS: %s\033[0m\n" "$OS"
        exit 1
        ;;
esac

case "$ARCH" in
    arm64|aarch64)
        ARCH_TARGET="aarch64"
        ;;
    x86_64|amd64)
        ARCH_TARGET="x86_64"
        ;;
    *)
        printf "\033[1;31m✖ Unsupported architecture: %s\033[0m\n" "$ARCH"
        exit 1
        ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"
mkdir -p "$INSTALL_DIR"

# 1. Try downloading pre-built binary from GitHub Releases first
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/lmem-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

INSTALLED_BIN=0
if [ "${LIGHTMEM_FROM_SOURCE:-0}" != "1" ]; then
    printf "  • Checking for pre-built binary (%s)...\n" "$TARGET"
    if curl -fsSL "$RELEASE_URL" -o "$TMP_DIR/lmem.tar.gz" 2>/dev/null; then
        tar -xzf "$TMP_DIR/lmem.tar.gz" -C "$TMP_DIR"
        mv "$TMP_DIR/lmem" "$INSTALL_DIR/lmem"
        chmod +x "$INSTALL_DIR/lmem"
        INSTALLED_BIN=1
        printf "\033[1;32m✔ Installed pre-built lmem to %s/lmem\033[0m\n" "$INSTALL_DIR"
    fi
fi

# 2. Fallback to cargo install --git if no release tarball or --from-source requested
if [ "$INSTALLED_BIN" -eq 0 ]; then
    printf "  • Building standalone binary from source via Cargo...\n"
    if ! command -v cargo >/dev/null 2>&1; then
        printf "  • Rust/Cargo not found. Installing rustup toolchain...\n"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    cargo install --git "https://github.com/${REPO}.git" --force
    if [ -f "$HOME/.cargo/bin/lmem" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
        cp "$HOME/.cargo/bin/lmem" "$INSTALL_DIR/lmem"
    fi
    printf "\033[1;32m✔ Built and installed lmem binary!\033[0m\n"
fi

# 3. Provision Native Needle 3 C-FFI Engine (libneedle + needle3.cact) with zero Python
if [ ! -f "$NEEDLE_CACHE_DIR/$NEEDLE_LIB" ] || [ ! -f "$NEEDLE_CACHE_DIR/needle3.cact" ]; then
    if [ ! -f "$HOME/.cache/cactus-needle/v3/3.0.1/$NEEDLE_LIB" ]; then
        printf "  • Provisioning Native Needle 3 C-FFI engine (%s + needle3.cact)...\n" "$NEEDLE_LIB"
        mkdir -p "$NEEDLE_CACHE_DIR"
        WHL_NAME="$(curl -fsSL https://huggingface.co/api/models/Cactus-Compute/needle3/tree/main/python 2>/dev/null | grep -oE "cactus_needle-[^\"]*${NEEDLE_WHL_PATTERN}" | head -n 1 || true)"
        if [ -n "$WHL_NAME" ]; then
            if curl -fsSL "https://huggingface.co/Cactus-Compute/needle3/resolve/main/python/${WHL_NAME}" -o "$TMP_DIR/needle.whl"; then
                unzip -q -j "$TMP_DIR/needle.whl" "needle/${NEEDLE_LIB}" -d "$NEEDLE_CACHE_DIR" 2>/dev/null || true
            fi
        fi
        if [ ! -f "$NEEDLE_CACHE_DIR/needle3.cact" ]; then
            curl -fsSL "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact" -o "$NEEDLE_CACHE_DIR/needle3.cact" || true
        fi
        if [ -f "$NEEDLE_CACHE_DIR/$NEEDLE_LIB" ] && [ -f "$NEEDLE_CACHE_DIR/needle3.cact" ]; then
            printf "\033[1;32m✔ Provisioned Native Needle 3 C-FFI engine at %s\033[0m\n" "$NEEDLE_CACHE_DIR"
        fi
    fi
else
    printf "\033[1;32m✔ Native Needle 3 C-FFI engine already present.\033[0m\n"
fi

printf "\n\033[1;32m🎉 LightMem installation complete!\033[0m\n"
printf "   Run: \033[1;36mlmem stats\033[0m or \033[1;36mlmem remember \"...\"\033[0m\n"
