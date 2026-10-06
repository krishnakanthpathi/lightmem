#!/usr/bin/env python3
"""
Packages LightMem Python distribution with precompiled native binary bundled directly inside.
"""
from __future__ import annotations
import sys
import shutil
import zipfile
import platform
import subprocess
from pathlib import Path


def build_platform_wheel(binary_path: str, platform_tag: str | None = None) -> Path:
    python_dir = Path(__file__).resolve().parent
    repo_root = python_dir.parent
    src_bin = python_dir / "src" / "lmem" / "bin"
    src_bin.mkdir(parents=True, exist_ok=True)

    src_binary = Path(binary_path).resolve()
    if not src_binary.is_file():
        raise FileNotFoundError(f"Binary not found at {src_binary}")

    exe_name = "lmem.exe" if platform.system() == "Windows" else "lmem"
    target_exe = src_bin / exe_name
    shutil.copy2(src_binary, target_exe)
    target_exe.chmod(0o755)

    if platform.system() != "Windows":
        subprocess.run(["strip", "-x", str(target_exe)], check=False)

    dist_dir = python_dir / "dist"
    dist_dir.mkdir(parents=True, exist_ok=True)

    # Clean existing matching wheels before build
    for old_whl in dist_dir.glob("lmem-*.whl"):
        old_whl.unlink()

    # Build base wheel via build
    subprocess.run(
        [sys.executable, "-m", "build", "--wheel", "--outdir", str(dist_dir)],
        cwd=str(python_dir),
        check=True,
    )

    base_whl = next(dist_dir.glob("lmem-*-py3-none-any.whl"))

    if not platform_tag:
        system = platform.system().lower()
        machine = platform.machine().lower()
        if system == "darwin":
            platform_tag = "macosx_11_0_arm64" if machine in ["arm64", "aarch64"] else "macosx_10_9_x86_64"
        elif system == "linux":
            platform_tag = "manylinux2014_aarch64" if machine in ["arm64", "aarch64"] else "manylinux2014_x86_64"
        elif system == "windows":
            platform_tag = "win_amd64"
        else:
            platform_tag = "any"

    if platform_tag == "any":
        return base_whl

    target_whl = dist_dir / base_whl.name.replace("py3-none-any.whl", f"py3-none-{platform_tag}.whl")

    # Re-tag WHEEL file inside the zip archive
    with zipfile.ZipFile(base_whl, "r") as zin, zipfile.ZipFile(target_whl, "w") as zout:
        for item in zin.infolist():
            content = zin.read(item.filename)
            if item.filename.endswith("WHEEL"):
                content = content.replace(b"Tag: py3-none-any", f"Tag: py3-none-{platform_tag}".encode())
            zout.writestr(item, content)

    base_whl.unlink(missing_ok=True)
    return target_whl


if __name__ == "__main__":
    binary = sys.argv[1] if len(sys.argv) > 1 else str(Path(__file__).resolve().parent.parent / "target" / "release" / "lmem")
    tag = sys.argv[2] if len(sys.argv) > 2 else None
    out = build_platform_wheel(binary, tag)
    print(f"Built platform wheel: {out}")
