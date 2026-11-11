from __future__ import annotations
import os
import sys
import shutil
import platform
import struct
import subprocess
import urllib.request
import tarfile
import zipfile
from pathlib import Path
from typing import Optional, List, Tuple


def _is_native_binary(path: Path | str) -> bool:
    """
    Checks if a file exists, is executable, matches current OS & CPU architecture,
    and is not a script. Safely handles permission errors and restricted system dirs.
    """
    try:
        p = Path(path)
        if not p.is_file() or not os.access(p, os.X_OK):
            return False

        with open(p, "rb") as f:
            header = f.read(64)
            if len(header) < 4:
                return False

            # Explicitly reject shell/python wrapper scripts starting with '#!'
            if header[:2] == b"#!":
                return False

            sys_name = platform.system().lower()
            machine = platform.machine().lower()

            if sys_name == "linux":
                # Must be an ELF binary
                if header[:4] != b"\x7fELF" or len(header) < 20:
                    return False
                endian = "<" if header[5] == 1 else ">"
                e_machine = struct.unpack(f"{endian}H", header[18:20])[0]
                # Match architecture:
                # 0x3E (62)  = x86_64 / amd64
                # 0xB7 (183) = aarch64 / arm64
                # 0x28 (40)  = arm (32-bit)
                # 0x03 (3)   = x86 (32-bit)
                if machine in ("x86_64", "amd64"):
                    return e_machine == 0x3E
                elif machine in ("arm64", "aarch64"):
                    return e_machine == 0xB7
                elif machine.startswith("arm"):
                    return e_machine in (0x28, 0xB7)
                elif machine in ("i386", "i686", "x86"):
                    return e_machine in (0x03, 0x3E)
                return True

            elif sys_name == "darwin":
                magic = header[:4]
                # Fat binary (universal macOS)
                if magic in (b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"):
                    return True
                # Mach-O 64-bit LE / BE
                if magic not in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf") or len(header) < 8:
                    return False
                cputype = struct.unpack("<I", header[4:8])[0]
                # CPU_TYPE_X86_64 = 0x01000007, CPU_TYPE_ARM64 = 0x0100000C
                if machine in ("arm64", "aarch64"):
                    return cputype in (0x0100000C, 0x01000007)
                elif machine in ("x86_64", "amd64"):
                    return cputype == 0x01000007
                return True

            elif sys_name == "windows":
                if header[:2] != b"MZ" or len(header) < 64:
                    return False
                pe_offset = struct.unpack("<I", header[60:64])[0]
                f.seek(pe_offset)
                pe_sig = f.read(4)
                if pe_sig != b"PE\0\0":
                    return False
                pe_machine = struct.unpack("<H", f.read(2))[0]
                # 0x8664 = AMD64 / x86_64, 0xAA64 = ARM64, 0x014C = i386
                if machine in ("x86_64", "amd64"):
                    return pe_machine in (0x8664, 0x014C)
                elif machine in ("arm64", "aarch64"):
                    return pe_machine in (0xAA64, 0x014C, 0x8664)
                return True

    except (PermissionError, OSError, Exception):
        return False

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
    env_path = os.environ.get("LMEM_BINARY_PATH") or os.environ.get("LIGHTMEM_BINARY_PATH")
    if env_path and _is_native_binary(env_path):
        return env_path

    # Check bundled binary packaged directly with the Python wheel (site-packages/lmem/bin/lmem)
    bundled = Path(__file__).parent / "bin" / ("lmem.exe" if platform.system() == "Windows" else "lmem")
    if _is_native_binary(bundled):
        return str(bundled)

    home = Path.home()
    candidate_paths = [
        home / ".lightmem" / "bin" / "lmem",
        home / ".lightmem" / "bin" / "lmem.exe",
        home / ".local" / "bin" / "lmem",
        home / ".cargo" / "bin" / "lmem",
    ]

    for p in candidate_paths:
        try:
            if _is_native_binary(p):
                return str(p)
        except (PermissionError, OSError):
            continue

    # Check PATH, but skip if it's a Python script/wrapper or permission-denied
    for path_dir in os.environ.get("PATH", "").split(os.pathsep):
        if not path_dir:
            continue
        try:
            candidate = Path(path_dir) / ("lmem.exe" if platform.system() == "Windows" else "lmem")
            if _is_native_binary(candidate):
                return str(candidate)
        except (PermissionError, OSError):
            continue

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
        "  curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh\n"
        "Or set the LMEM_BINARY_PATH environment variable."
    )


def ensure_binary_downloaded(version: str = "v0.7.2") -> str:
    """Downloads pre-built release binary for current system into ~/.lightmem/bin/lmem."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        target = "aarch64-apple-darwin" if machine in ["arm64", "aarch64"] else "x86_64-apple-darwin"
        archive_name = f"lmem-{target}.tar.gz"
        exe_name = "lmem"
    elif system == "linux":
        is_android = (
            os.path.exists("/system/bin/linker64")
            or "com.termux" in os.environ.get("PREFIX", "")
            or "android" in platform.uname().release.lower()
        )
        if is_android and machine in ["arm64", "aarch64"]:
            target = "aarch64-linux-android"
        else:
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
        raise RuntimeError(f"Failed to auto-download lmem binary from {url}: {e}")


def execute_lmem(
    args: List[str],
    input_text: Optional[str] = None,
    timeout: Optional[float] = None,
) -> Tuple[int, str, str]:
    binary = get_binary_path()
    try:
        proc = subprocess.Popen(
            [binary] + args,
            stdin=subprocess.PIPE if input_text is not None else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError as e:
        raise RuntimeError(
            f"Failed to execute native binary at '{binary}' on {platform.system()} {platform.machine()}: {e}. "
            "The binary may be corrupted or built for an incompatible architecture. "
            "Please reinstall via 'curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh' "
            "or set LMEM_BINARY_PATH to a compatible binary."
        ) from e
    try:
        stdout, stderr = proc.communicate(input=input_text, timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        raise
    return proc.returncode, stdout, stderr
