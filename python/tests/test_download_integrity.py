"""Release downloads must not publish corrupt or path-traversing archives."""
import hashlib
import io
import stat
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch
from pathlib import Path
from lmem.binary import _extract_checked, _verify_release_digest, ensure_binary_downloaded


class DownloadIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "output"
        self.output.mkdir()

    def test_digest_requires_exactly_one_matching_entry_and_matching_content(self):
        archive = self.root / "lmem-test.zip"
        archive.write_bytes(b"synthetic archive")
        sums = self.root / "SHA256SUMS"
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        sums.write_text(f"{digest}  {archive.name}\n")
        _verify_release_digest(archive, sums)
        archive.write_bytes(b"corrupted archive")
        with self.assertRaises(ValueError):
            _verify_release_digest(archive, sums)
        sums.write_text(f"{digest}  {archive.name}\n{digest}  {archive.name}\n")
        with self.assertRaises(ValueError):
            _verify_release_digest(archive, sums)

    def test_zip_rejects_traversal_windows_paths_and_symbolic_links(self):
        archive = self.root / "unsafe.zip"
        for filename in ["../escape", "/absolute", "C:/escape", "..\\escape"]:
            with zipfile.ZipFile(archive, "w") as z:
                z.writestr(filename, b"must not escape")
            with self.assertRaises(ValueError):
                _extract_checked(archive, self.output)
        link = zipfile.ZipInfo("link")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(archive, "w") as z:
            z.writestr(link, "../escape")
        with self.assertRaises(ValueError):
            _extract_checked(archive, self.output)
        self.assertFalse((self.root / "escape").exists())

    def test_tar_rejects_hard_links_before_following_them(self):
        archive = self.root / "unsafe.tar.gz"
        with tarfile.open(archive, "w:gz") as t:
            link = tarfile.TarInfo("link")
            link.type = tarfile.LNKTYPE
            link.linkname = "../escape"
            t.addfile(link)
        with self.assertRaises(ValueError):
            _extract_checked(archive, self.output)

    def test_normal_release_layout_extracts_only_inside_staging(self):
        archive = self.root / "valid.tar.gz"
        with tarfile.open(archive, "w:gz") as t:
            file = tarfile.TarInfo("./lmem")
            file.size = 6
            t.addfile(file, io.BytesIO(b"binary"))
        _extract_checked(archive, self.output)
        self.assertEqual((self.output / "lmem").read_bytes(), b"binary")

    def test_offline_missing_executable_does_not_attempt_a_download(self):
        with patch.dict("os.environ", {"LIGHTMEM_OFFLINE": "1"}), patch("lmem.binary.Path.home", return_value=self.root), patch("lmem.binary.urllib.request.urlretrieve") as download:
            with self.assertRaisesRegex(RuntimeError, "Offline mode"):
                ensure_binary_downloaded()
            download.assert_not_called()


if __name__ == "__main__":
    unittest.main()
