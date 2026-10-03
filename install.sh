#!/usr/bin/env sh
# LightMem Universal Single-Command Installer (macOS & Linux)
# Usage: curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

set -e

REPO="krishnakanthpathi/lightmem"
VERSION="v0.1.0"
INSTALL_DIR="${LIGHTMEM_INSTALL_DIR:-$HOME/.local/bin}"
NEEDLE_CACHE_DIR="$HOME/.cache/cactus-needle/v3/3.1.0"

OS="$(uname -s)"
ARCH="$(uname -m)"

# Render 24-bit TrueColor Crimson Cloud Logo + Installer Header
awk -v os="$OS" -v arch="$ARCH" -v ver="$VERSION" 'BEGIN {
    m[0]  = "                     WWWWWW           ";
    m[1]  = "                   WWWRRRRWWW         ";
    m[2]  = "                  WRRRRRRRRRWW        ";
    m[3]  = "              WWWWRRRRRRRRRRRWWW      ";
    m[4]  = "             WWRRWRRRRRWWRRRRWWWWWW   ";
    m[5]  = "            WRRRWWRRRRRRWRRRRRRRRRWW  ";
    m[6]  = "           WWRRRWWRRRRRRWRRRRRRRRRRWW ";
    m[7]  = "           WRRRRRWRRRRRRWRRRRRRRRRRRW ";
    m[8]  = "           WRRRRRRWWWRWWWRRRRRRRRRRRWW";
    m[9]  = "        WWWWRRRRRRRRWWWRRRRRRRRRRRRRRW";
    m[10] = "W      WWRRRRRRRRRRRRRRRRRRRRRRRRRRRRW";
    m[11] = "WWW  WWRRRRRRRRRRRRRRRRRRRRWWWWRRRRRRW";
    m[12] = "WWWWWWRRRRRRRRRRRRRRRRRRRRWWRRRRRRRRWW";
    m[13] = " WRRRRRRRRRRRRRRRRRRRRRRRRWRRRRRRRRRW ";
    m[14] = " WWRRRRRRRRWWWWWRRRRRRRRRRWRRRRRRRRWW ";
    m[15] = "  WRRRRRRRWRRRRWWRRRRRRRRRWWRRRRRRWW  ";
    m[16] = "   WRRRRRWRRRRRRWRRRRRRRRRRWWRRRRWW   ";
    m[17] = "    WRRRRWRRRRRRRRRRRRRRRRRRWWWWWW    ";
    m[18] = "     WWWWWWRRRRRRRRRRRRRRRRRRW        ";
    m[19] = "          WRRRRRRRRWRRRRRRRRW         ";
    m[20] = "          WWRRRRRWWWWWRRRWWW          ";
    m[21] = "            WWWWWW   WWWWW            ";

    r[0]  = "";
    r[1]  = "\033[1;38;2;220;38;38m❖\033[0m  \033[1;38;2;248;250;252mL I G H T M E M\033[0m  \033[38;2;113;113;122m" ver "\033[0m";
    r[2]  = "\033[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m";
    r[3]  = "\033[1;38;2;220;38;38m▸\033[0m \033[38;2;228;228;231mUniversal Standalone Installer\033[0m";
    r[4]  = "  \033[38;2;113;113;122m◫ Target     \033[1;38;2;248;250;252m" os " (" arch ")\033[0m";
    r[5]  = "  \033[38;2;113;113;122m◈ Engines    \033[1;38;2;167;139;250monnx\033[0m \033[38;2;113;113;122m+\033[0m \033[1;38;2;220;38;38mneedle-3 FFI\033[0m";
    r[6]  = "  \033[38;2;113;113;122m✦ Storage    \033[38;2;212;212;216mSQLite WAL + FTS5 + Vector\033[0m";
    r[7]  = "\033[38;2;113;113;122m────────────────────────────────────────────────\033[0m";
    r[8]  = "";
    r[9]  = "";
    r[10] = "";

    printf "\n";
    for (p = 0; p < 11; p++) {
        top = m[p*2];
        bot = m[p*2+1];
        line = "  ";
        for (x = 1; x <= 38; x++) {
            tc = substr(top, x, 1);
            bc = substr(bot, x, 1);
            t_rgb = (tc == "W") ? "248;250;252" : ((tc == "R") ? "204;36;36" : "");
            b_rgb = (bc == "W") ? "248;250;252" : ((bc == "R") ? "204;36;36" : "");
            if (t_rgb == "" && b_rgb == "") {
                line = line " ";
            } else if (t_rgb != "" && b_rgb == "") {
                line = line "\033[38;2;" t_rgb "m▀\033[0m";
            } else if (t_rgb == "" && b_rgb != "") {
                line = line "\033[38;2;" b_rgb "m▄\033[0m";
            } else if (t_rgb == b_rgb) {
                line = line "\033[38;2;" t_rgb "m█\033[0m";
            } else {
                line = line "\033[38;2;" t_rgb ";48;2;" b_rgb "m▀\033[0m";
            }
        }
        printf "%s   %s\n", line, r[p];
    }
    printf "\n";
}'

# Unbuffered live percentage downloader (polls actual bytes written on disk every 0.15s)
download_with_pct() {
    url="$1"
    dest="$2"
    label="$3"
    expected_bytes="${4:-12500000}"
    rm -f "$dest"
    printf "  \033[1;31m▸\033[0m %s... \033[1;31m1%%\033[0m" "$label"

    curl -fsSL --happy-eyeballs-timeout-ms 200 --connect-timeout 3 "$url" -o "$dest" 2>/dev/null &
    dl_pid=$!

    warmup_pct=1
    while kill -0 "$dl_pid" 2>/dev/null; do
        if [ -f "$dest" ]; then
            cur_bytes="$(wc -c < "$dest" 2>/dev/null | tr -d ' ')"
            if [ -n "$cur_bytes" ] && [ "$cur_bytes" -gt 0 ] && [ "$expected_bytes" -gt 0 ]; then
                pct=$(( cur_bytes * 100 / expected_bytes ))
                if [ "$pct" -lt "$warmup_pct" ]; then pct="$warmup_pct"; fi
                if [ "$pct" -gt 99 ]; then pct=99; fi
                printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%d%%\033[0m   " "$label" "$pct"
            fi
        else
            if [ "$warmup_pct" -lt 5 ]; then
                warmup_pct=$(( warmup_pct + 1 ))
                printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%d%%\033[0m   " "$label" "$warmup_pct"
            fi
        fi
        sleep 0.15
    done

    if wait "$dl_pid" && [ -s "$dest" ]; then
        printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$label"
        return 0
    else
        printf "\r  \033[1;33m▸\033[0m %s... unavailable\n" "$label"
        return 1
    fi
}

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

# 1. Download pre-built binary from GitHub Releases
RELEASE_URL="https://github.com/${REPO}/releases/download/${VERSION}/lmem-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

INSTALLED_BIN=0
if [ "${LIGHTMEM_FROM_SOURCE:-0}" != "1" ]; then
    if download_with_pct "$RELEASE_URL" "$TMP_DIR/lmem.tar.gz" "Downloading pre-built binary (${TARGET})" 12467430; then
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

# 3. Provision Native Needle 3 C-FFI Engine (libneedle + needle3.cact)
if [ ! -f "$NEEDLE_CACHE_DIR/$NEEDLE_LIB" ] && [ ! -f "$HOME/.cache/cactus-needle/v3/3.0.1/$NEEDLE_LIB" ]; then
    mkdir -p "$NEEDLE_CACHE_DIR"
    if download_with_pct "https://huggingface.co/Cactus-Compute/needle3/resolve/main/python/${NEEDLE_WHL}" "$TMP_DIR/needle.whl" "Downloading Needle 3 C-FFI runtime (${NEEDLE_LIB})" 530108; then
        unzip -q -j "$TMP_DIR/needle.whl" "needle/${NEEDLE_LIB}" -d "$NEEDLE_CACHE_DIR" 2>/dev/null || true
    fi
fi

if [ ! -f "$NEEDLE_CACHE_DIR/needle3.cact" ] && [ ! -f "$HOME/.cache/cactus-needle/v3/3.0.1/needle3.cact" ]; then
    mkdir -p "$NEEDLE_CACHE_DIR"
    download_with_pct "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact" "$NEEDLE_CACHE_DIR/needle3.cact" "Downloading Needle 3 model weights (needle3.cact)" 35500000 || true
fi

# 4. Pre-warm ONNX embedding model (~/.lightmem/models)
ONNX_LABEL="Verifying ONNX embedding model"
printf "  \033[1;31m▸\033[0m %s... \033[1;31m1%%\033[0m" "$ONNX_LABEL"
"$INSTALL_DIR/lmem" recall "init" --limit 1 --json >/dev/null 2>&1 &
onnx_pid=$!
while kill -0 "$onnx_pid" 2>/dev/null; do
    if [ -d "$HOME/.lightmem/models" ]; then
        cur_kb="$(du -sk "$HOME/.lightmem/models" 2>/dev/null | awk '{print $1}')"
        if [ -n "$cur_kb" ]; then
            pct=$(( cur_kb * 100 / 130000 ))
            if [ "$pct" -lt 1 ]; then pct=1; fi
            if [ "$pct" -gt 99 ]; then pct=99; fi
            printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%d%%\033[0m   " "$ONNX_LABEL" "$pct"
        fi
    fi
    sleep 0.15
done
wait "$onnx_pid" || true
printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$ONNX_LABEL"

printf "\n  \033[1;31m❖\033[0m \033[1;37mLightMem installation complete!\033[0m Run: \033[1;31mlmem\033[0m\n\n"
