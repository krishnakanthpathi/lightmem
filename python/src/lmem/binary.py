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


def _is_native_binary(path: Path | str) -> bool:
    """Checks if a file exists, is executable, and is a true native binary (not a script)."""
    p = Path(path)
    if not p.is_file() or not os.access(p, os.X_OK):
        return False
    try:
        with open(p, "rb") as f:
            header = f.read(4)
            # Explicitly reject shell/python wrapper scripts starting with '#!'
            if header[:2] == b"#!":
                return False
            # Check known native binary signatures:
            # macOS Mach-O: \xcf\xfa\xed\xfe (64-bit LE), \xfe\xed\xfa\xcf (64-bit BE), \xca\xfe\xba\xbe (fat)
            # Linux ELF: \x7fELF
            # Windows PE: MZ
            if (
                header in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\x7fELF")
                or header[:2] == b"MZ"
            ):
                return True
    except Exception:
        pass
    return False


def get_binary_path() -> str:
    """
    Locates the `lmem` native executable.
    Checks:
    1. Environment variable `LMEM_BINARY_PATH` or `LIGHTMEM_BINARY_PATH`
    2. Well-known local install directories (~/.lightmem/bin, ~/.local/bin, ~/.cargo/bin)
    3. PATH, filtering out Python wrapper scripts
    4. Auto-downloads prebuilt binary to `~/.lightmem/bin/lmem` if missing.
    """
    # 0. Check bundled binary packaged directly with the Python wheel (site-packages/lmem/bin/lmem)
    bundled = Path(__file__).parent / "bin" / ("lmem.exe" if platform.system() == "Windows" else "lmem")
    if _is_native_binary(bundled):
        return str(bundled)

    env_path = os.environ.get("LMEM_BINARY_PATH") or os.environ.get("LIGHTMEM_BINARY_PATH")
    if env_path and _is_native_binary(env_path):
        return env_path

    home = Path.home()
    candidate_paths = [
        home / ".lightmem" / "bin" / "lmem",
        home / ".lightmem" / "bin" / "lmem.exe",
        home / ".local" / "bin" / "lmem",
        home / ".cargo" / "bin" / "lmem",
    ]

    for p in candidate_paths:
        if _is_native_binary(p):
            return str(p)

    # Check PATH, but skip if it's a Python script/wrapper
    for path_dir in os.environ.get("PATH", "").split(os.pathsep):
        if not path_dir:
            continue
        candidate = Path(path_dir) / ("lmem.exe" if platform.system() == "Windows" else "lmem")
        if _is_native_binary(candidate):
            return str(candidate)

    # If not found, attempt to auto-download to ~/.lightmem/bin/lmem
    try:
        downloaded = ensure_binary_downloaded()
        if downloaded and _is_native_binary(downloaded):
            return downloaded
    except Exception as e:
        pass

    raise FileNotFoundError(
        "Could not find the 'lmem' native binary on this system. "
        "Install it via:\n"
        "  curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/neural-reranker/install.sh | sh\n"
        "Or set the LMEM_BINARY_PATH environment variable."
    )


def ensure_binary_downloaded(version: str = "v0.2.3") -> str:
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

    if _is_native_binary(dest_exe):
        return str(dest_exe)

    url = f"https://github.com/krishnakanthpathi/lightmem/releases/download/{version}/{archive_name}"
    tmp_archive = dest_dir / archive_name

    sys.stderr.write(f"Downloading LightMem native engine ({version}) from GitHub...\n")
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
        sys.stderr.write("LightMem native engine successfully installed.\n")
        return str(dest_exe)
    except Exception as e:
        tmp_archive.unlink(missing_ok=True)
        # Fall back to v0.2.2 if v0.2.3 is still building on GitHub Actions
        if version != "v0.2.2":
            try:
                return ensure_binary_downloaded(version="v0.2.2")
            except Exception:
                pass
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
