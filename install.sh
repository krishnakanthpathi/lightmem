#!/usr/bin/env sh
# LightMem Universal Single-Command Installer (macOS & Linux)
# Usage: curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

set -e

REPO="krishnakanthpathi/lightmem"
VERSION="${LIGHTMEM_VERSION:-v0.8.4}"
INSTALL_DIR="${LIGHTMEM_INSTALL_DIR:-$HOME/.local/bin}"

old_tty=""
if [ -r /dev/tty ] && [ -w /dev/tty ] && stty -g </dev/tty >/dev/null 2>&1; then
    old_tty="$(stty -g </dev/tty)"
fi
TMP_DIR=""

cleanup() {
    exit_code=$?
    sig="${1:-}"
    trap - INT TERM EXIT
    if [ -n "${old_tty:-}" ]; then
        stty "$old_tty" </dev/tty 2>/dev/null || true
    fi
    if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
    case "$sig" in
        INT) exit 130 ;;
        TERM) exit 143 ;;
        *) exit "$exit_code" ;;
    esac
}
trap 'cleanup INT' INT
trap 'cleanup TERM' TERM
trap 'cleanup EXIT' EXIT

# Support CLI flags & environment variables
SKIP_MODELS=0
if [ "${LIGHTMEM_SKIP_MODELS:-0}" = "1" ] || [ "${LIGHTMEM_SKIP_MODELS:-}" = "true" ] || [ "${LIGHTMEM_NO_MODELS:-0}" = "1" ] || [ "${LIGHTMEM_MODEL:-}" = "none" ] || [ "${LIGHTMEM_MODEL:-}" = "skip" ]; then
    SKIP_MODELS=1
fi

for arg in "$@"; do
    case "$arg" in
        --uninstall|-u)
            SCRIPT_DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo "")"
            if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/uninstall.sh" ]; then
                exec "$SCRIPT_DIR/uninstall.sh" "$@"
            else
                exec sh -c "$(curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/uninstall.sh")" sh "$@"
            fi
            ;;
        --skip-models|--no-models|--skip-local-models|--skip-lockout-models)
            SKIP_MODELS=1
            ;;
    esac
done

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
    r[5]  = "  \033[38;2;113;113;122m◈ Engines    \033[1;38;2;167;139;250monnx (bge-small)\033[0m \033[38;2;113;113;122m+\033[0m \033[1;38;2;220;38;38monnx (minilm-squad2)\033[0m";
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
    expected_bytes="${4:-14600000}"
    rm -f "$dest"
    printf "  \033[1;31m▸\033[0m %s... \033[1;31m1%%\033[0m" "$label"

    (curl -4 -fsSL --retry 3 --connect-timeout 10 "$url" -o "$dest" 2>/dev/null || curl -fsSL --retry 3 --connect-timeout 10 "$url" -o "$dest" 2>/dev/null) &
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

# Run a command with an animated braille spinner (or clean line if not a tty)
run_with_spinner() {
    msg="$1"
    shift
    if [ ! -t 1 ]; then
        printf "  \033[1;31m▸\033[0m %s...\n" "$msg"
        if "$@"; then
            printf "  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m\n" "$msg"
            return 0
        else
            return 1
        fi
    fi

    "$@" >/dev/null 2>&1 &
    cmd_pid=$!
    spin_chars="⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏"
    while kill -0 "$cmd_pid" 2>/dev/null; do
        for c in $spin_chars; do
            if ! kill -0 "$cmd_pid" 2>/dev/null; then
                break
            fi
            printf "\r  \033[1;31m%s\033[0m %s...   " "$c" "$msg"
            sleep 0.08
        done
    done
    wait "$cmd_pid"
    res=$?
    if [ "$res" -eq 0 ]; then
        printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$msg"
        return 0
    else
        printf "\r  \033[1;33m▸\033[0m %s... \033[1;33mskipped\033[0m   \n" "$msg"
        return "$res"
    fi
}

case "$OS" in
    Darwin)
        OS_TARGET="apple-darwin"
        ;;
    Linux)
        # Auto-detect Android / Termux environment
        if [ -n "${TERMUX_VERSION:-}" ] || [ "$(uname -o 2>/dev/null)" = "Android" ] || [ -d "/data/data/com.termux" ] || [ -f "/system/bin/app_process" ]; then
            OS_TARGET="linux-android"
        else
            OS_TARGET="unknown-linux-gnu"
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

INSTALLED_BIN=0
verify_release_archive() {
    (curl -4 -fsSL --retry 2 --connect-timeout 8 "https://github.com/${REPO}/releases/download/${VERSION}/SHA256SUMS" -o "$TMP_DIR/SHA256SUMS" 2>/dev/null || \
     curl -fsSL --retry 2 --connect-timeout 8 "https://github.com/${REPO}/releases/download/${VERSION}/SHA256SUMS" -o "$TMP_DIR/SHA256SUMS" 2>/dev/null) || return 1
    release_expected="$(awk -v file="lmem-${TARGET}.tar.gz" '$2 == file || $2 == "*" file { print $1 }' "$TMP_DIR/SHA256SUMS")"
    [ "${#release_expected}" -eq 64 ] || return 1
    case "$release_expected" in *[!a-fA-F0-9]*) return 1 ;; esac
    if command -v sha256sum >/dev/null 2>&1; then
        release_actual="$(sha256sum "$TMP_DIR/lmem.tar.gz" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        release_actual="$(shasum -a 256 "$TMP_DIR/lmem.tar.gz" | awk '{print $1}')"
    else
        return 1
    fi
    [ "$release_actual" = "$release_expected" ]
}

install_binary_files() {
    tar -xzf "$TMP_DIR/lmem.tar.gz" -C "$TMP_DIR" || return 1
    mv "$TMP_DIR/lmem" "$INSTALL_DIR/lmem" || return 1
    chmod +x "$INSTALL_DIR/lmem" || return 1
    if [ -d "$HOME/.cargo/bin" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
        cp "$INSTALL_DIR/lmem" "$HOME/.cargo/bin/lmem" 2>/dev/null || true
        chmod +x "$HOME/.cargo/bin/lmem" 2>/dev/null || true
    fi
    if [ -n "${PREFIX:-}" ] && [ -d "$PREFIX/bin" ] && [ "$INSTALL_DIR" != "$PREFIX/bin" ]; then
        cp "$INSTALL_DIR/lmem" "$PREFIX/bin/lmem" 2>/dev/null || true
        chmod +x "$PREFIX/bin/lmem" 2>/dev/null || true
    fi
    return 0
}

if [ "${LIGHTMEM_FROM_SOURCE:-0}" != "1" ]; then
    if download_with_pct "$RELEASE_URL" "$TMP_DIR/lmem.tar.gz" "Downloading pre-built binary (${TARGET})" 14600000; then
        if run_with_spinner "Verifying release checksum (SHA-256)" verify_release_archive && \
           run_with_spinner "Extracting and installing binary" install_binary_files; then
            INSTALLED_BIN=1
        fi
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
    cargo install --git "https://github.com/${REPO}.git" --tag "$VERSION" --locked --force
    if [ -f "$HOME/.cargo/bin/lmem" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
        cp "$HOME/.cargo/bin/lmem" "$INSTALL_DIR/lmem"
    fi
    printf "  \033[1;32m◈\033[0m Built and installed lmem binary... \033[1;32m100%%\033[0m\n"
fi

# 3. Use the same short setup prompts as the CLI. Explicit environment
# choices remain available to automated installers; otherwise do not change config.
MODEL_CHOICE="${LIGHTMEM_MODEL:-}"
LLM_CHOICE="${LIGHTMEM_LLM:-}"
if [ -n "$LLM_CHOICE" ]; then
    case "$LLM_CHOICE" in
        1|ollama|ollama-local) setup_preset="ollama-local" ;;
        2|cloud|ollama-cloud) setup_preset="ollama-cloud" ;;
        4|offline|local) setup_preset="local" ;;
        demo|hash) setup_preset="demo" ;;
        3|custom)
            printf "Custom model setup: run lmem setup and choose LLM answers.\n"
            setup_preset=""
            ;;
        *) printf "Unknown LIGHTMEM_LLM choice; run lmem setup.\n" >&2; exit 1 ;;
    esac
    if [ -n "$setup_preset" ]; then
        "$INSTALL_DIR/lmem" setup --preset "$setup_preset" --entity-extraction off --yes
    fi
elif [ "$SKIP_MODELS" != "1" ] && [ -z "$MODEL_CHOICE" ] && [ -r /dev/tty ] && [ -w /dev/tty ]; then
    if "$INSTALL_DIR/lmem" setup </dev/tty >/dev/tty 2>/dev/tty; then
        :
    else
        printf "Setup was not completed. Run lmem setup when ready.\n"
    fi
else
    printf "Configure when ready: lmem setup (default: local search and answers).\n"
fi

# Download only models explicitly requested by an installer environment option.
if [ "$SKIP_MODELS" = "1" ]; then
    printf "Local model downloads skipped. Prepare later: lmem config --download all\n"
elif [ -n "$MODEL_CHOICE" ]; then
    case "$MODEL_CHOICE" in
        1|bge-small) embedding_choice="bge-small" ;;
        2|minilm) embedding_choice="minilm" ;;
        3|nomic) embedding_choice="nomic" ;;
        4|all) embedding_choice="bge-small" ;;
        *) printf "Unknown LIGHTMEM_MODEL choice.\n" >&2; exit 1 ;;
    esac
    "$INSTALL_DIR/lmem" config --backend onnx --onnx-model "$embedding_choice" --yes
    if [ "$MODEL_CHOICE" = "4" ] || [ "$MODEL_CHOICE" = "all" ]; then
        "$INSTALL_DIR/lmem" config --download all
    else
        "$INSTALL_DIR/lmem" config --download "$embedding_choice"
        "$INSTALL_DIR/lmem" config --download minilm-squad2
    fi
fi

# 6. Install Shell Completions with Interactive Tab + Arrow-Key Menu Navigation
configure_shell_completions() {
    COMP_DIR="$HOME/.lightmem/completions"
    mkdir -p "$COMP_DIR"
    "$INSTALL_DIR/lmem" completions zsh > "$COMP_DIR/_lmem" 2>/dev/null || true
    "$INSTALL_DIR/lmem" completions bash > "$COMP_DIR/lmem.bash" 2>/dev/null || true

    if [ -f "$HOME/.zshrc" ] || [ "${SHELL:-}" = "/bin/zsh" ] || [ "${SHELL:-}" = "/usr/bin/zsh" ]; then
        touch "$HOME/.zshrc"
        if ! grep -q "lightmem/completions" "$HOME/.zshrc" 2>/dev/null; then
            cat << 'EOF' >> "$HOME/.zshrc"

# LightMem CLI completions & interactive Tab/Arrow-key menu
fpath=("$HOME/.lightmem/completions" $fpath)
autoload -Uz compinit && compinit -C
zstyle ':completion:*' menu select
EOF
        fi
    fi

    if [ -f "$HOME/.bashrc" ] || [ "${SHELL:-}" = "/bin/bash" ] || [ "${SHELL:-}" = "/usr/bin/bash" ]; then
        touch "$HOME/.bashrc"
        if ! grep -q "lightmem/completions/lmem.bash" "$HOME/.bashrc" 2>/dev/null; then
            cat << 'EOF' >> "$HOME/.bashrc"

# LightMem CLI completions
[ -f "$HOME/.lightmem/completions/lmem.bash" ] && source "$HOME/.lightmem/completions/lmem.bash"
EOF
        fi
    fi
}
run_with_spinner "Configuring shell completions (Tab + Arrow keys)" configure_shell_completions

# 7. Auto-deploy LightMem skill to detected AI agent platforms
deploy_agent_skills() {
    "$INSTALL_DIR/lmem" connect >/dev/null 2>&1 || true
}
run_with_spinner "Deploying LightMem skill to AI agent environments" deploy_agent_skills

# 8. Interactive Model Context Protocol (MCP) Setup (Optional)
select_mcp_interactive() {
    printf "\n  \033[1;31m❖\033[0m \033[1;37mConfigure Model Context Protocol (MCP) for AI Clients?\033[0m\n" >/dev/tty
    printf "    \033[38;2;148;163;184mEnables Claude Desktop, Cursor, and Antigravity to use LightMem tools natively.\033[0m\n" >/dev/tty
    printf "  \033[1;31m▸\033[0m Configure MCP in detected client configs? \033[38;2;148;163;184m[y/N, default=N]\033[0m: " >/dev/tty
    read -r MCP_CHOICE </dev/tty || MCP_CHOICE="N"
    printf "\n" >/dev/tty
}

MCP_CHOICE="${LIGHTMEM_MCP:-}"
if [ -z "$MCP_CHOICE" ] && [ -r /dev/tty ] && [ -w /dev/tty ]; then
    select_mcp_interactive || MCP_CHOICE="N"
fi
[ -z "$MCP_CHOICE" ] && MCP_CHOICE="N"

case "$MCP_CHOICE" in
    y|Y|yes|YES)
        "$INSTALL_DIR/lmem" connect --mcp >/dev/null 2>&1 || true
        printf "  \033[1;32m◈\033[0m Configured LightMem MCP in detected client configs... \033[1;32m100%%\033[0m\n"
        ;;
    *)
        printf "  \033[38;2;113;113;122m↷ Skipped MCP configuration (run 'lmem connect --mcp' anytime to enable)\033[0m\n"
        ;;
esac

printf "\n  \033[1;31m❖\033[0m \033[1;37mLightMem installation complete!\033[0m Run: \033[1;31mlmem\033[0m\n\n"
