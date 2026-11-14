"""Run only installer setup selection with a recording binary, never the installer."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipIf(os.name == 'nt', 'POSIX shell setup tests')
class InstallerSetup(unittest.TestCase):
    def run_setup(self, *, skip='0', **overrides):
        script = (ROOT / 'install.sh').read_text().split('# 3. Use the same short setup prompts', 1)[1].split('# 6. Install Shell', 1)[0]
        script = '# 3. Use the same short setup prompts' + script
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            binary = path / 'lmem'
            log = path / 'calls'
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$LMEM_SETUP_LOG"\n')
            binary.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if not k.startswith('LIGHTMEM_')}
            env.update(INSTALL_DIR=str(path), SKIP_MODELS=skip, LMEM_SETUP_LOG=str(log), **overrides)
            result = subprocess.run(['sh', '-eu', '-c', script], env=env, text=True, capture_output=True, timeout=5)
            return result, log.read_text().splitlines() if log.exists() else []

    def test_skip_model_installation_does_not_change_configuration(self):
        result, calls = self.run_setup(skip='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [])

    def test_explicit_embedding_choice_uses_the_requested_model(self):
        result, calls = self.run_setup(LIGHTMEM_MODEL='minilm')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            'config --backend onnx --onnx-model minilm --yes',
            'config --download minilm', 'config --download minilm-squad2',
        ])

    def test_explicit_cloud_answers_do_not_enable_entity_extraction(self):
        result, calls = self.run_setup(skip='1', LIGHTMEM_LLM='cloud')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, ['setup --preset ollama-cloud --entity-extraction off --yes'])

    def test_invalid_requested_model_fails_without_any_native_action(self):
        result, calls = self.run_setup(LIGHTMEM_MODEL='invalid-model')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])


if __name__ == '__main__':
    unittest.main()
