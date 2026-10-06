#!/usr/bin/env sh
# LightMem Universal Uninstaller (macOS & Linux)
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/uninstall.sh | sh
# Options:
#   --purge-data, -a    Also delete memories database (~/.lightmem/memories.db) and downloaded models
#   --yes, -y           Non-interactive mode (proceed without confirmation)

set -e

PURGE_DATA=0
ASSUME_YES=0

for arg in "$@"; do
    case "$arg" in
        --purge-data|-a|--all)
            PURGE_DATA=1
            ;;
        --yes|-y)
            ASSUME_YES=1
            ;;
        --help|-h)
            printf "Usage: uninstall.sh [OPTIONS]\n\n"
            printf "Options:\n"
            printf "  -y, --yes          Non-interactive mode (proceed without prompts)\n"
            printf "  -a, --purge-data   Purge ~/.lightmem completely (deletes memories.db and cached models)\n"
            printf "  -h, --help         Show this help message\n"
            exit 0
            ;;
    esac
done

# Render Header
printf "\n"
printf "  \033[1;38;2;220;38;38m❖\033[0m  \033[1;38;2;248;250;252mL I G H T M E M\033[0m  \033[38;2;113;113;122mUninstaller\033[0m\n"
printf "  \033[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m\n"
printf "  \033[1;38;2;220;38;38m▸\033[0m \033[38;2;228;228;231mRemoving LightMem engine, binaries & shell completions\033[0m\n"
printf "  \033[38;2;113;113;122m────────────────────────────────────────────────\033[0m\n\n"

if [ "$ASSUME_YES" -eq 0 ] && [ -r /dev/tty ] && [ -w /dev/tty ]; then
    printf "  \033[1;33m? Proceed with uninstalling LightMem? [y/N]:\033[0m " >/dev/tty
    read -r CONFIRM </dev/tty || CONFIRM="n"
    case "$CONFIRM" in
        [yY]|[yY][eE][sS])
            ;;
        *)
            printf "\n  \033[38;2;113;113;122mAborted by user.\033[0m\n\n"
            exit 0
            ;;
    esac
    printf "\n"
fi

# 1. Remove standalone binaries
REMOVED_BINS=0
for bin_path in \
    "$HOME/.local/bin/lmem" \
    "$HOME/.cargo/bin/lmem" \
    "$HOME/.lightmem/bin/lmem" \
    "$HOME/.lightmem/bin"
do
    if [ -e "$bin_path" ]; then
        rm -rf "$bin_path"
        printf "  \033[1;32m◈\033[0m Removed binary: \033[38;2;212;212;216m%s\033[0m\n" "$bin_path"
        REMOVED_BINS=$(( REMOVED_BINS + 1 ))
    fi
done

if [ "$REMOVED_BINS" -eq 0 ]; then
    printf "  \033[38;2;113;113;122m▸ No standalone binaries found in ~/.local/bin or ~/.cargo/bin\033[0m\n"
fi

# 2. Check and remove Python pip package
if command -v pip3 >/dev/null 2>&1 && pip3 show lmem >/dev/null 2>&1; then
    printf "  \033[1;31m▸\033[0m Uninstalling Python package (pip3 uninstall lmem)...\n"
    pip3 uninstall -y lmem >/dev/null 2>&1 || true
    printf "  \033[1;32m◈\033[0m Uninstalled lmem Python package\n"
elif command -v python3 >/dev/null 2>&1 && python3 -m pip show lmem >/dev/null 2>&1; then
    printf "  \033[1;31m▸\033[0m Uninstalling Python package (python3 -m pip uninstall lmem)...\n"
    python3 -m pip uninstall -y lmem >/dev/null 2>&1 || true
    printf "  \033[1;32m◈\033[0m Uninstalled lmem Python package\n"
fi

# 3. Clean up shell completions
if [ -d "$HOME/.lightmem/completions" ]; then
    rm -rf "$HOME/.lightmem/completions"
    printf "  \033[1;32m◈\033[0m Removed completion directory: \033[38;2;212;212;216m~/.lightmem/completions\033[0m\n"
fi

# Clean ~/.zshrc
if [ -f "$HOME/.zshrc" ]; then
    if grep -q "lightmem/completions" "$HOME/.zshrc" 2>/dev/null; then
        # Create backup
        cp "$HOME/.zshrc" "$HOME/.zshrc.lightmem.bak"
        # Remove lightmem blocks
        awk '
            /# LightMem CLI completions/ { skip = 1; next }
            skip && /fpath=\("\$HOME\/\.lightmem\/completions"/ { next }
            skip && /autoload -Uz compinit/ { next }
            skip && /zstyle.*completion.*menu/ { skip = 0; next }
            skip && /^$/ { skip = 0; next }
            !skip { print }
        ' "$HOME/.zshrc.lightmem.bak" > "$HOME/.zshrc"
        rm -f "$HOME/.zshrc.lightmem.bak"
        printf "  \033[1;32m◈\033[0m Cleaned completion hooks from \033[38;2;212;212;216m~/.zshrc\033[0m\n"
    fi
fi

# Clean ~/.bashrc
if [ -f "$HOME/.bashrc" ]; then
    if grep -q "lightmem/completions" "$HOME/.bashrc" 2>/dev/null; then
        cp "$HOME/.bashrc" "$HOME/.bashrc.lightmem.bak"
        awk '
            /# LightMem CLI completions/ { skip = 1; next }
            skip && /lightmem\/completions\/lmem\.bash/ { next }
            skip && /show-all-if-ambiguous/ { next }
            skip && /menu-complete/ { next }
            skip && /^$/ { skip = 0; next }
            !skip { print }
        ' "$HOME/.bashrc.lightmem.bak" > "$HOME/.bashrc"
        rm -f "$HOME/.bashrc.lightmem.bak"
        printf "  \033[1;32m◈\033[0m Cleaned completion hooks from \033[38;2;212;212;216m~/.bashrc\033[0m\n"
    fi
fi

# Purge zsh completion caches
rm -f "$HOME"/.zcompdump* 2>/dev/null || true

# 4. Handle vault and models directory
if [ "$PURGE_DATA" -eq 1 ]; then
    if [ -d "$HOME/.lightmem" ]; then
        rm -rf "$HOME/.lightmem"
        printf "  \033[1;31m◈\033[0m Purged all data & models: \033[38;2;212;212;216m~/.lightmem\033[0m\n"
    fi
else
    if [ -f "$HOME/.lightmem/memories.db" ]; then
        printf "\n  \033[1;36mℹ\033[0m \033[38;2;228;228;231mPersistent vault preserved at:\033[0m \033[1m~/.lightmem/memories.db\033[0m\n"
        printf "    \033[38;2;113;113;122m(To delete your memories vault as well, pass: --purge-data)\033[0m\n"
    fi
fi

printf "\n  \033[1;32m✔\033[0m \033[1;37mLightMem has been successfully uninstalled.\033[0m\n"
printf "    \033[38;2;113;113;122mRestart your shell terminal or run 'rehash' to complete.\033[0m\n\n"
