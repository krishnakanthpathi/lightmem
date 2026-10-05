from __future__ import annotations
import os
import sys
import shutil
import platform
import subprocess
import urllib.request
import tarfile
import zipfile
from pathlib import Path
from typing import Optional, List, Tuple


def get_binary_path() -> str:
    """
    Locates the `lmem` executable.
    Checks:
    1. Environment variable `LMEM_BINARY_PATH` or `LIGHTMEM_BINARY_PATH`
    2. PATH (`which lmem`)
    3. User local directory `~/.local/bin/lmem` or `~/.cargo/bin/lmem`
    4. Auto-downloads prebuilt binary to `~/.lightmem/bin/lmem` if missing.
    """
    env_path = os.environ.get("LMEM_BINARY_PATH") or os.environ.get("LIGHTMEM_BINARY_PATH")
    if env_path and os.path.isfile(env_path) and os.access(env_path, os.X_OK):
        return env_path

    which_path = shutil.which("lmem")
    if which_path:
        return which_path

    home = Path.home()
    candidate_paths = [
        home / ".local" / "bin" / "lmem",
        home / ".cargo" / "bin" / "lmem",
        home / ".lightmem" / "bin" / "lmem",
        home / ".lightmem" / "bin" / "lmem.exe",
    ]

    for p in candidate_paths:
        if p.is_file() and os.access(str(p), os.X_OK):
            return str(p)

    # If not found, attempt to auto-download to ~/.lightmem/bin/lmem
    try:
        downloaded = ensure_binary_downloaded()
        if downloaded and os.path.isfile(downloaded) and os.access(downloaded, os.X_OK):
            return downloaded
    except Exception as e:
        pass

    raise FileNotFoundError(
        "Could not find the 'lmem' native binary on this system. "
        "Install it via:\n"
        "  curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/neural-reranker/install.sh | sh\n"
        "Or set the LMEM_BINARY_PATH environment variable."
    )


def ensure_binary_downloaded(version: str = "v0.2.0") -> str:
    """Downloads pre-built release binary for current system into ~/.lightmem/bin/lmem."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        target = "aarch64-apple-darwin" if machine in ["arm64", "aarch64"] else "x86_64-apple-darwin"
        archive_name = f"lmem-{target}.tar.gz"
        exe_name = "lmem"
    elif system == "linux":
        target = "aarch64-unknown-linux-gnu" if machine in ["arm64", "aarch64"] else "x86_64-unknown-linux-gnu"
        archive_name = f"lmem-{target}.tar.gz"
        exe_name = "lmem"
    elif system == "windows":
        target = "x86_64-pc-windows-msvc"
        archive_name = f"lmem-{target}.zip"
        exe_name = "lmem.exe"
    else:
        raise RuntimeError(f"Unsupported OS: {system}")

    dest_dir = Path.home() / ".lightmem" / "bin"
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest_exe = dest_dir / exe_name

    if dest_exe.is_file() and os.access(str(dest_exe), os.X_OK):
        return str(dest_exe)

    url = f"https://github.com/krishnakanthpathi/lightmem/releases/download/{version}/{archive_name}"
    tmp_archive = dest_dir / archive_name

    try:
        urllib.request.urlretrieve(url, tmp_archive)
        if archive_name.endswith(".zip"):
            with zipfile.ZipFile(tmp_archive, "r") as z:
                z.extractall(dest_dir)
        else:
            with tarfile.open(tmp_archive, "r:gz") as t:
                t.extractall(dest_dir)

        tmp_archive.unlink(missing_ok=True)
        dest_exe.chmod(0o755)
        return str(dest_exe)
    except Exception as e:
        tmp_archive.unlink(missing_ok=True)
        raise RuntimeError(f"Failed to auto-download lmem binary from {url}: {e}")


def execute_lmem(args: List[str], input_text: Optional[str] = None) -> Tuple[int, str, str]:
    binary = get_binary_path()
    proc = subprocess.Popen(
        [binary] + args,
        stdin=subprocess.PIPE if input_text is not None else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    stdout, stderr = proc.communicate(input=input_text)
    return proc.returncode, stdout, stderr
