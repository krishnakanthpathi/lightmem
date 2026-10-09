#!/usr/bin/env sh
# LightMem Universal Single-Command Installer (macOS & Linux)
# Usage: curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

set -e

REPO="krishnakanthpathi/lightmem"
VERSION="v0.2.7"
INSTALL_DIR="${LIGHTMEM_INSTALL_DIR:-$HOME/.local/bin}"

# Support --uninstall / -u flag
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

case "$OS" in
    Darwin)
        OS_TARGET="apple-darwin"
        ;;
    Linux)
        OS_TARGET="unknown-linux-gnu"
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
    cargo install --git "https://github.com/${REPO}.git" --branch main --locked --force
    if [ -f "$HOME/.cargo/bin/lmem" ] && [ "$INSTALL_DIR" != "$HOME/.cargo/bin" ]; then
        cp "$HOME/.cargo/bin/lmem" "$INSTALL_DIR/lmem"
    fi
    printf "  \033[1;32m◈\033[0m Built and installed lmem binary... \033[1;32m100%%\033[0m\n"
fi

# 3. Interactive ONNX Embedding & QA Model Selection & Pre-warming (~/.lightmem/models)
download_onnx_model() {
    model_alias="$1"
    expected_kb="$2"
    label="Downloading ONNX model (${model_alias})"
    base_kb=0
    if [ -d "$HOME/.lightmem/models" ]; then
        base_kb="$(du -sk "$HOME/.lightmem/models" 2>/dev/null | awk '{print $1}')"
        [ -z "$base_kb" ] && base_kb=0
    fi
    printf "  \033[1;31m▸\033[0m %s... \033[1;31m1%%\033[0m" "$label"
    "$INSTALL_DIR/lmem" config --download "$model_alias" >/dev/null 2>&1 &
    onnx_pid=$!
    tick=1
    while kill -0 "$onnx_pid" 2>/dev/null; do
        if [ -d "$HOME/.lightmem/models" ]; then
            cur_kb="$(du -sk "$HOME/.lightmem/models" 2>/dev/null | awk '{print $1}')"
            if [ -n "$cur_kb" ]; then
                delta_kb=$(( cur_kb - base_kb ))
                if [ "$delta_kb" -lt 0 ]; then delta_kb=0; fi
                pct=$(( delta_kb * 100 / expected_kb ))
                if [ "$pct" -le "$tick" ] && [ "$tick" -lt 5 ]; then
                    tick=$(( tick + 1 ))
                    pct="$tick"
                fi
                if [ "$pct" -lt 1 ]; then pct=1; fi
                if [ "$pct" -gt 99 ]; then pct=99; fi
                printf "\r  \033[1;31m▸\033[0m %s... \033[1;31m%d%%\033[0m   " "$label" "$pct"
            fi
        fi
        sleep 0.15
    done
    wait "$onnx_pid" || true
    printf "\r  \033[1;32m◈\033[0m %s... \033[1;32m100%%\033[0m   \n" "$label"
}

select_model_interactive() {
    sel=1
    printf "\n  \033[1;31m❖\033[0m \033[1;37mSelect ONNX Embedding Model(s) to Download\033[0m \033[38;2;148;163;184m(Use ↑/↓ arrows or 1-4, then Enter):\033[0m\n" >/dev/tty
    render_menu() {
        idx=1
        for item in \
            "bge-small  ─ Xenova/bge-small-en-v1.5       (384-dim · ~130 MB · Default)" \
            "minilm     ─ Xenova/all-MiniLM-L6-v2        (384-dim ·  ~90 MB · Ultra-Fast)" \
            "nomic      ─ nomic-ai/nomic-embed-text-v1.5 (768-dim · ~520 MB · High-Capacity)" \
            "all        ─ Download all 3 models (switch offline anytime via lmem config)"
        do
            name="${item%% *}"
            rest="${item#* }"
            if [ "$idx" -eq "$sel" ]; then
                printf "\r\033[2K    \033[1;31m▸ [%d]\033[0m \033[1;37m%s\033[0m \033[38;2;248;250;252m%s\033[0m\n" "$idx" "$name" "$rest" >/dev/tty
            else
                printf "\r\033[2K      \033[38;2;148;163;184m[%d] %s %s\033[0m\n" "$idx" "$name" "$rest" >/dev/tty
            fi
            idx=$(( idx + 1 ))
        done
    }

    if stty -g </dev/tty >/dev/null 2>&1; then
        old_tty="$(stty -g </dev/tty)"
        render_menu
        while :; do
            stty -icanon -echo min 1 time 0 </dev/tty 2>/dev/null || break
            key="$(dd bs=1 count=1 </dev/tty 2>/dev/null)"
            stty "$old_tty" </dev/tty 2>/dev/null || true
            case "$key" in
                ""|"$(printf '\r')"|"$(printf '\n')")
                    break
                    ;;
                1|2|3|4)
                    sel="$key"
                    printf "\033[4A" >/dev/tty
                    render_menu
                    break
                    ;;
                "$(printf '\033')")
                    stty -icanon -echo min 0 time 1 </dev/tty 2>/dev/null || true
                    seq="$(dd bs=2 count=1 </dev/tty 2>/dev/null)"
                    stty "$old_tty" </dev/tty 2>/dev/null || true
                    case "$seq" in
                        "[A"|"OA")
                            sel=$(( sel - 1 ))
                            [ "$sel" -lt 1 ] && sel=4
                            ;;
                        "[B"|"OB")
                            sel=$(( sel + 1 ))
                            [ "$sel" -gt 4 ] && sel=1
                            ;;
                    esac
                    printf "\033[4A" >/dev/tty
                    render_menu
                    ;;
            esac
        done
        stty "$old_tty" </dev/tty 2>/dev/null || true
        printf "\n" >/dev/tty
        MODEL_CHOICE="$sel"
    else
        render_menu
        printf "  \033[1;31m▸\033[0m Enter choice \033[38;2;148;163;184m[1-4, default=1]\033[0m: " >/dev/tty
        read -r MODEL_CHOICE </dev/tty || MODEL_CHOICE="1"
        printf "\n" >/dev/tty
    fi
}

MODEL_CHOICE="${LIGHTMEM_MODEL:-}"
if [ -z "$MODEL_CHOICE" ] && [ -r /dev/tty ] && [ -w /dev/tty ]; then
    select_model_interactive || MODEL_CHOICE="1"
fi

case "$MODEL_CHOICE" in
    2|minilm)
        download_onnx_model "minilm" 90000
        "$INSTALL_DIR/lmem" config --backend onnx --onnx-model minilm --yes >/dev/null 2>&1 || true
        ;;
    3|nomic)
        download_onnx_model "nomic" 520000
        "$INSTALL_DIR/lmem" config --backend onnx --onnx-model nomic --yes >/dev/null 2>&1 || true
        ;;
    4|all)
        download_onnx_model "bge-small" 130000
        download_onnx_model "minilm" 90000
        download_onnx_model "nomic" 520000
        "$INSTALL_DIR/lmem" config --backend onnx --onnx-model bge-small --yes >/dev/null 2>&1 || true
        ;;
    *)
        download_onnx_model "bge-small" 130000
        "$INSTALL_DIR/lmem" config --backend onnx --onnx-model bge-small --yes >/dev/null 2>&1 || true
        ;;
esac

# 4. Pre-warm ONNX Extractive QA Model (minilm-squad2)
download_onnx_model "minilm-squad2" 128000

# 5. Interactive LLM Engine Setup (Default for Ingestion & QA)
select_llm_interactive() {
    sel=1
    printf "\n  \033[1;31m❖\033[0m \033[1;37mConfigure LLM Engine for Memory Ingestion & QA\033[0m \033[38;2;148;163;184m(Use ↑/↓ arrows or 1-4, then Enter):\033[0m\n" >/dev/tty
    render_llm_menu() {
        idx=1
        for item in \
            "ollama-local ─ Local Ollama (http://localhost:11434 · Auto-detects models · Recommended)" \
            "ollama-cloud ─ Ollama Cloud (https://ollama.com · gemma4:31b-cloud)" \
            "custom       ─ Custom OpenAI-compatible / Remote LLM endpoint (URL + Model)" \
            "offline      ─ 100% Offline (Local ONNX QA + deterministic NLP · No LLM)"
        do
            name="${item%% *}"
            rest="${item#* }"
            if [ "$idx" -eq "$sel" ]; then
                printf "\r\033[2K    \033[1;31m▸ [%d]\033[0m \033[1;37m%s\033[0m \033[38;2;248;250;252m%s\033[0m\n" "$idx" "$name" "$rest" >/dev/tty
            else
                printf "\r\033[2K      \033[38;2;148;163;184m[%d] %s %s\033[0m\n" "$idx" "$name" "$rest" >/dev/tty
            fi
            idx=$(( idx + 1 ))
        done
    }

    if stty -g </dev/tty >/dev/null 2>&1; then
        old_tty="$(stty -g </dev/tty)"
        render_llm_menu
        while :; do
            stty -icanon -echo min 1 time 0 </dev/tty 2>/dev/null || break
            key="$(dd bs=1 count=1 </dev/tty 2>/dev/null)"
            stty "$old_tty" </dev/tty 2>/dev/null || true
            case "$key" in
                ""|"$(printf '\r')"|"$(printf '\n')")
                    break
                    ;;
                1|2|3|4)
                    sel="$key"
                    printf "\033[4A" >/dev/tty
                    render_llm_menu
                    break
                    ;;
                "$(printf '\033')")
                    stty -icanon -echo min 0 time 1 </dev/tty 2>/dev/null || true
                    seq="$(dd bs=2 count=1 </dev/tty 2>/dev/null)"
                    stty "$old_tty" </dev/tty 2>/dev/null || true
                    case "$seq" in
                        "[A"|"OA")
                            sel=$(( sel - 1 ))
                            [ "$sel" -lt 1 ] && sel=4
                            ;;
                        "[B"|"OB")
                            sel=$(( sel + 1 ))
                            [ "$sel" -gt 4 ] && sel=1
                            ;;
                    esac
                    printf "\033[4A" >/dev/tty
                    render_llm_menu
                    ;;
            esac
        done
        stty "$old_tty" </dev/tty 2>/dev/null || true
        printf "\n" >/dev/tty
        LLM_CHOICE="$sel"
    else
        render_llm_menu
        printf "  \033[1;31m▸\033[0m Enter choice \033[38;2;148;163;184m[1-4, default=1]\033[0m: " >/dev/tty
        read -r LLM_CHOICE </dev/tty || LLM_CHOICE="1"
        printf "\n" >/dev/tty
    fi
}

LLM_CHOICE="${LIGHTMEM_LLM:-}"
if [ -z "$LLM_CHOICE" ] && [ -r /dev/tty ] && [ -w /dev/tty ]; then
    select_llm_interactive || LLM_CHOICE="1"
fi
[ -z "$LLM_CHOICE" ] && LLM_CHOICE="1"

case "$LLM_CHOICE" in
    2|cloud|ollama-cloud)
        printf "  \033[1;31m▸\033[0m Enter Ollama Cloud model \033[38;2;148;163;184m[default=gemma4:31b-cloud]\033[0m: " >/dev/tty
        read -r CLOUD_MODEL </dev/tty || CLOUD_MODEL=""
        [ -z "$CLOUD_MODEL" ] && CLOUD_MODEL="gemma4:31b-cloud"
        "$INSTALL_DIR/lmem" config --preset ollama-cloud --reranker "ollama:$CLOUD_MODEL" --yes >/dev/null 2>&1 || true
        printf "  \033[1;32m◈\033[0m Configured Ollama Cloud (%s)... \033[1;32m100%%\033[0m\n" "$CLOUD_MODEL"
        ;;
    3|custom)
        printf "  \033[1;31m▸\033[0m Enter LLM endpoint URL \033[38;2;148;163;184m[default=http://localhost:11434]\033[0m: " >/dev/tty
        read -r CUSTOM_URL </dev/tty || CUSTOM_URL=""
        [ -z "$CUSTOM_URL" ] && CUSTOM_URL="http://localhost:11434"
        printf "  \033[1;31m▸\033[0m Enter Model name \033[38;2;148;163;184m[e.g. qwen2.5:3b, llama3.2:3b]\033[0m: " >/dev/tty
        read -r CUSTOM_MODEL </dev/tty || CUSTOM_MODEL=""
        [ -z "$CUSTOM_MODEL" ] && CUSTOM_MODEL="qwen2.5:3b"
        "$INSTALL_DIR/lmem" config --url "$CUSTOM_URL" --reranker "ollama:$CUSTOM_MODEL" --yes >/dev/null 2>&1 || true
        printf "  \033[1;32m◈\033[0m Configured custom endpoint (%s · %s)... \033[1;32m100%%\033[0m\n" "$CUSTOM_URL" "$CUSTOM_MODEL"
        ;;
    4|offline|local)
        "$INSTALL_DIR/lmem" config --preset local --yes >/dev/null 2>&1 || true
        printf "  \033[1;32m◈\033[0m Configured 100%% offline local ONNX mode... \033[1;32m100%%\033[0m\n"
        ;;
    *)
        # 1. Local Ollama (Default & Recommended)
        "$INSTALL_DIR/lmem" config --preset ollama-local --yes >/dev/null 2>&1 || true
        if curl -s -m 2 http://localhost:11434/api/tags >/dev/null 2>&1; then
            printf "  \033[1;32m◈\033[0m Configured Local Ollama (active & connected at http://localhost:11434)... \033[1;32m100%%\033[0m\n"
        else
            printf "  \033[1;33m▸\033[0m Configured Local Ollama (http://localhost:11434 · start with 'ollama serve')... \033[1;32m100%%\033[0m\n"
        fi
        ;;
esac

# 6. Install Shell Completions with Interactive Tab + Arrow-Key Menu Navigation
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

# LightMem CLI completions & interactive Tab menu
[ -f "$HOME/.lightmem/completions/lmem.bash" ] && source "$HOME/.lightmem/completions/lmem.bash"
bind 'set show-all-if-ambiguous on' 2>/dev/null || true
bind '"\t": menu-complete' 2>/dev/null || true
bind '"\e[Z": menu-complete-backward' 2>/dev/null || true
EOF
    fi
fi
printf "  \033[1;32m◈\033[0m Configured shell completions (Tab + Arrow keys)... \033[1;32m100%%\033[0m\n"

printf "\n  \033[1;31m❖\033[0m \033[1;37mLightMem installation complete!\033[0m Run: \033[1;31mlmem\033[0m\n\n"
