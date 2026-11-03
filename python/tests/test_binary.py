import errno
import os
import platform
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch, mock_open

from lmem.binary import _is_native_binary, get_binary_path, execute_lmem


class TestBinaryResolution(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.tmp = Path(self.temp_dir.name)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_rejects_scripts(self):
        script_file = self.tmp / "dummy.sh"
        script_file.write_bytes(b"#!/usr/bin/env bash\necho hello\n")
        script_file.chmod(0o755)
        self.assertFalse(_is_native_binary(script_file))

    def test_handles_permission_error_gracefully(self):
        # Simulates restricted system directories like /system/xbin/lmem on Android/Termux
        restricted_file = self.tmp / "restricted_lmem"
        restricted_file.write_bytes(b"\x7fELF" + b"\x00" * 60)

        with patch("builtins.open", side_effect=PermissionError(errno.EACCES, "Permission denied")):
            self.assertFalse(_is_native_binary(restricted_file))

        with patch("os.access", side_effect=PermissionError(errno.EACCES, "Permission denied")):
            self.assertFalse(_is_native_binary(restricted_file))

    def test_architecture_validation_linux_x86_64(self):
        # Construct ELF headers
        # Byte 0-3: \x7fELF, Byte 4: 2 (64-bit), Byte 5: 1 (LE)
        # Offset 18-20: e_machine
        # 0x3E (62) for x86_64, 0xB7 (183) for aarch64
        elf_x86_64 = bytearray(b"\x7fELF\x02\x01" + b"\x00" * 12 + struct.pack("<H", 0x3E) + b"\x00" * 44)
        elf_arm64 = bytearray(b"\x7fELF\x02\x01" + b"\x00" * 12 + struct.pack("<H", 0xB7) + b"\x00" * 44)

        f_x86 = self.tmp / "lmem_x86"
        f_x86.write_bytes(elf_x86_64)
        f_x86.chmod(0o755)

        f_arm = self.tmp / "lmem_arm"
        f_arm.write_bytes(elf_arm64)
        f_arm.chmod(0o755)

        with patch("platform.system", return_value="Linux"), patch("platform.machine", return_value="x86_64"):
            self.assertTrue(_is_native_binary(f_x86))
            # Must reject ARM binary on x86_64 / AMD processor!
            self.assertFalse(_is_native_binary(f_arm))

        with patch("platform.system", return_value="Linux"), patch("platform.machine", return_value="aarch64"):
            self.assertFalse(_is_native_binary(f_x86))
            self.assertTrue(_is_native_binary(f_arm))

    def test_architecture_validation_darwin(self):
        # Mach-O ARM64: magic 0xcffaedfe, cputype 0x0100000C
        macho_arm64 = bytearray(b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + b"\x00" * 56)
        # Mach-O x86_64: magic 0xcffaedfe, cputype 0x01000007
        macho_x86 = bytearray(b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x01000007) + b"\x00" * 56)

        f_mac_arm = self.tmp / "macho_arm"
        f_mac_arm.write_bytes(macho_arm64)
        f_mac_arm.chmod(0o755)

        f_mac_x86 = self.tmp / "macho_x86"
        f_mac_x86.write_bytes(macho_x86)
        f_mac_x86.chmod(0o755)

        with patch("platform.system", return_value="Darwin"), patch("platform.machine", return_value="arm64"):
            self.assertTrue(_is_native_binary(f_mac_arm))
            # x86_64 allowed on Darwin arm64 via Rosetta
            self.assertTrue(_is_native_binary(f_mac_x86))

        with patch("platform.system", return_value="Linux"), patch("platform.machine", return_value="x86_64"):
            # Mach-O must NOT be recognized as valid on Linux!
            self.assertFalse(_is_native_binary(f_mac_arm))
            self.assertFalse(_is_native_binary(f_mac_x86))

    def test_get_binary_path_skips_incompatible_or_permission_denied_candidates(self):
        unreadable_dir = self.tmp / "restricted"
        unreadable_dir.mkdir()
        unreadable_file = unreadable_dir / "lmem"
        unreadable_file.touch()

        def fake_is_native(p):
            if str(p).startswith(str(unreadable_dir)):
                raise PermissionError(errno.EACCES, "Permission denied")
            return False

        with patch.dict(os.environ, {"PATH": str(unreadable_dir)}):
            with patch("lmem.binary._is_native_binary", side_effect=fake_is_native):
                with patch("lmem.binary.ensure_binary_downloaded", return_value=None):
                    with self.assertRaises(FileNotFoundError):
                        get_binary_path()

    def test_execute_lmem_descriptive_error_on_exec_format_failure(self):
        with patch("lmem.binary.get_binary_path", return_value="/usr/local/bin/lmem"):
            with patch("subprocess.Popen", side_effect=OSError(errno.ENOEXEC, "Exec format error")):
                with self.assertRaises(RuntimeError) as ctx:
                    execute_lmem(["stats"])
                self.assertIn("incompatible architecture", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
