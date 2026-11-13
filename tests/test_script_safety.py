"""Run only the shell-cleanup section, never the uninstaller itself."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ShellCleanupTests(unittest.TestCase):
    def cleanup(self, shell, text):
        script = (ROOT / "uninstall.sh").read_text()
        section = script.split("# Clean ~/.zshrc\n", 1)[1].split("# Purge zsh completion caches", 1)[0]
        with tempfile.TemporaryDirectory() as home:
            path = Path(home) / shell
            path.write_text(text)
            result = subprocess.run(["sh", "-eu", "-c", section], env={**os.environ, "HOME": home}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            cleaned = path.read_text()
            backups = list(Path(home).glob(shell + ".lightmem.bak*"))
            self.assertTrue(backups, "retain a recoverable shell configuration backup")
            self.assertIn(text, [p.read_text() for p in backups])
            return cleaned

    def test_bash_preserves_adjacent_unrelated_settings(self):
        block = '# LightMem CLI completions\n[ -f "$HOME/.lightmem/completions/lmem.bash" ] && source "$HOME/.lightmem/completions/lmem.bash"\n'
        self.assertEqual(self.cleanup('.bashrc', 'export KEEP_BEFORE=1\n' + block + 'export KEEP_AFTER=1\n'), 'export KEEP_BEFORE=1\nexport KEEP_AFTER=1\n')

    def test_zsh_preserves_adjacent_unrelated_settings(self):
        block = '# LightMem CLI completions & interactive Tab/Arrow-key menu\nfpath=("$HOME/.lightmem/completions" $fpath)\nautoload -Uz compinit && compinit\n'
        self.assertEqual(self.cleanup('.zshrc', block + 'export KEEP_AFTER=1\n'), 'export KEEP_AFTER=1\n')

    def test_all_completion_file_can_become_empty(self):
        self.assertEqual(self.cleanup('.bashrc', '# LightMem CLI completions\n[ -f "$HOME/.lightmem/completions/lmem.bash" ] && source "$HOME/.lightmem/completions/lmem.bash"\n'), '')

    def test_similar_comments_are_not_block_markers(self):
        text = '# My LightMem CLI completions notes\nexport KEEP=1 # lightmem/completions\n'
        self.assertEqual(self.cleanup('.bashrc', text), text)


class WindowsInstallerContractTests(unittest.TestCase):
    def test_cargo_fallback_uses_requested_release_and_checks_exit(self):
        script = (ROOT / 'install.ps1').read_text()
        self.assertIn('--tag $ReleaseTag --locked --force', script)
        self.assertNotIn('--branch', script)
        self.assertIn('if ($LASTEXITCODE -ne 0)', script)
        self.assertIn('$ExpectedVersion', script)
        self.assertIn('Installed version mismatch', script)

    def test_every_native_command_has_exit_check(self):
        lines = (ROOT / 'install.ps1').read_text().splitlines()
        for i, line in enumerate(lines):
            if line.strip().startswith('& ') or '= & $ExePath' in line:
                self.assertIn('$LASTEXITCODE', lines[i + 1], line)


if __name__ == '__main__':
    unittest.main()
