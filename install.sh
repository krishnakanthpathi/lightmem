#!/usr/bin/env sh
# LightMem Universal Single-Command Installer (macOS & Linux)
# Usage: curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

set -e

REPO="krishnakanthpathi/lightmem"
INSTALL_DIR="${LIGHTMEM_INSTALL_DIR:-$HOME/.local/bin}"
NEEDLE_CACHE_DIR="$HOME/.cache/cactus-needle/v3/3.1.0"

printf "\033[1;31m❖\033[0m \033[1;37mInstalling LightMem (lmem)...\033[0m\n"

# Single-line live percentage downloader (0% -> 100%)
download_with_pct() {
    url="$1"
    dest="$2"
    label="$3"
    rm -f "$dest"
    printf "  \033[1;31m▸\033[0m %s... \033[1;31m0%%\033[0m" "$label"
    curl -fL "$url" -o "$dest" 2>&1 | tr '\r' '\n' | awk -v label="$label" '
        /^[[:space:]]*[0-9]+/ {
            if ($2 != "0" && $1 ~ /^[0-9]+$/) {
                printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%s%%\033[0m   ", label, $1;
            }
        }
    '
    if [ -s "$dest" ]; then
        printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$label"
        return 0
    else
        printf "\r  \033[1;33m▸\033[0m %s... unavailable\n" "$label"
        return 1
    fi
}

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        OS_TARGET="apple-darwin"
        NEEDLE_LIB="libneedle.dylib"
        if [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
            NEEDLE_WHL="cactus_needle-3.1.0-py3-none-macosx_11_0_arm64.whl"
        else
            NEEDLE_WHL="cactus_needle-3.1.0-py3-none-macosx_11_0_x86_64.whl"
        fi
        ;;
    Linux)
        OS_TARGET="unknown-linux-gnu"
        NEEDLE_LIB="libneedle.so"
        if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
            NEEDLE_WHL="cactus_needle-3.1.0-py3-none-manylinux2014_aarch64.whl"
        else
            NEEDLE_WHL="cactus_needle-3.1.0-py3-none-manylinux2014_x86_64.whl"
        fi
        ;;
    *)
        printf "\033[1;31m✕ Unsupported OS: %s\033[0m\n" "$OS"
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
        printf "\033[1;31m✕ Unsupported architecture: %s\033[0m\n" "$ARCH"
        exit 1
        ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"
mkdir -p "$INSTALL_DIR"

# 1. Download pre-built binary from GitHub Releases with percentage progress
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/lmem-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

INSTALLED_BIN=0
if [ "${LIGHTMEM_FROM_SOURCE:-0}" != "1" ]; then
    if download_with_pct "$RELEASE_URL" "$TMP_DIR/lmem.tar.gz" "Downloading pre-built binary (${TARGET})"; then
        tar -xzf "$TMP_DIR/lmem.tar.gz" -C "$TMP_DIR"
        mv "$TMP_DIR/lmem" "$INSTALL_DIR/lmem"
        chmod +x "$INSTALL_DIR/lmem"
        if [ -d "$HOME/.cargo/bin" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
            cp "$INSTALL_DIR/lmem" "$HOME/.cargo/bin/lmem"
            chmod +x "$HOME/.cargo/bin/lmem"
        fi
        INSTALLED_BIN=1
    fi
fi

# 2. Fallback to cargo install --git --locked if no pre-built release tarball is available
if [ "$INSTALLED_BIN" -eq 0 ]; then
    printf "  \033[1;33m▸\033[0m Building from source via Cargo...\n"
    if ! command -v cargo >/dev/null 2>&1; then
        printf "  \033[1;31m▸\033[0m Installing rustup toolchain...\n"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    cargo install --git "https://github.com/${REPO}.git" --locked --force
    if [ -f "$HOME/.cargo/bin/lmem" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
        cp "$HOME/.cargo/bin/lmem" "$INSTALL_DIR/lmem"
    fi
    printf "  \033[1;32m◈\033[0m Built and installed lmem binary... \033[1;32m100%%\033[0m\n"
fi

# 3. Provision Native Needle 3 C-FFI Engine (libneedle + needle3.cact) with percentage progress
if [ ! -f "$NEEDLE_CACHE_DIR/$NEEDLE_LIB" ] && [ ! -f "$HOME/.cache/cactus-needle/v3/3.0.1/$NEEDLE_LIB" ]; then
    mkdir -p "$NEEDLE_CACHE_DIR"
    if download_with_pct "https://huggingface.co/Cactus-Compute/needle3/resolve/main/python/${NEEDLE_WHL}" "$TMP_DIR/needle.whl" "Downloading Needle 3 C-FFI runtime (${NEEDLE_LIB})"; then
        unzip -q -j "$TMP_DIR/needle.whl" "needle/${NEEDLE_LIB}" -d "$NEEDLE_CACHE_DIR" 2>/dev/null || true
    fi
fi

if [ ! -f "$NEEDLE_CACHE_DIR/needle3.cact" ] && [ ! -f "$HOME/.cache/cactus-needle/v3/3.0.1/needle3.cact" ]; then
    mkdir -p "$NEEDLE_CACHE_DIR"
    download_with_pct "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact" "$NEEDLE_CACHE_DIR/needle3.cact" "Downloading Needle 3 model weights (needle3.cact)" || true
fi

# 4. Pre-warm ONNX embedding model (~/.lightmem/models) with percentage progress
ONNX_LABEL="Verifying ONNX embedding model"
printf "  \033[1;31m▸\033[0m %s... \033[1;31m0%%\033[0m" "$ONNX_LABEL"
"$INSTALL_DIR/lmem" recall "init" --limit 1 --json 2>&1 >/dev/null | tr '\r' '\n' | awk -v label="$ONNX_LABEL" '
    /MiB\// {
        if (match($0, /[0-9.]+\s*MiB\/[0-9.]+\s*MiB/)) {
            s = substr($0, RSTART, RLENGTH);
            split(s, parts, "/");
            cur = parts[1] + 0;
            tot = parts[2] + 0;
            if (tot > 0) {
                pct = int((cur / tot) * 100);
                if (pct > 100) pct = 100;
                printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%d%%\033[0m   ", label, pct;
            }
        }
    }
' || true
printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$ONNX_LABEL"

printf "\n\033[1;31m❖\033[0m \033[1;37mLightMem installation complete!\033[0m Run: \033[1;31mlmem\033[0m\n"
