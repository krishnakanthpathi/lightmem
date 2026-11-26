"""Exercise real line prompts in an isolated pseudo-terminal; never use user data."""
import errno
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get('LMEM_TEST_BINARY', ROOT / 'target/debug/lmem')).resolve()


@unittest.skipUnless(sys.platform != 'win32' and BINARY.exists(), 'requires a Unix PTY and built lmem')
class TerminalPrompts(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.config = self.root / 'config/config.json'
        self.db = self.root / 'vault.db'
        self.env = {k: v for k, v in os.environ.items() if k not in {
            'LIGHTMEM_DB', 'LIGHTMEM_CONNECTION', 'LIGHTMEM_REMOTE_URL', 'LIGHTMEM_URL',
            'LIGHTMEM_API_KEY', 'LIGHTMEM_KEY', 'OLLAMA_API_KEY', 'LMEM_OLLAMA_API_KEY',
        }}
        self.env.update(LIGHTMEM_CONFIG_DIR=str(self.config.parent), LIGHTMEM_OFFLINE='1', NO_COLOR='1')

    def tearDown(self):
        self.directory.cleanup()

    def demo(self):
        subprocess.run([str(BINARY), '--db', str(self.db), 'setup', '--preset', 'demo', '--yes', '--json'], env=self.env, cwd=self.root, check=True, capture_output=True)

    def interact(self, args, steps):
        import pty
        pid, master = pty.fork()
        if pid == 0:
            os.chdir(self.root)
            os.execve(str(BINARY), [str(BINARY), '--db', str(self.db), *args], self.env)
        data = b''
        consumed = 0
        deadline = time.monotonic() + 15
        status = None
        try:
            for expected, response in steps:
                marker = expected.encode()
                while marker not in data[consumed:]:
                    if time.monotonic() >= deadline:
                        self.fail(f'Timeout waiting for {expected!r}: {data.decode(errors="replace")}')
                    if select.select([master], [], [], .1)[0]:
                        try:
                            chunk = os.read(master, 65536)
                        except OSError as error:
                            if error.errno == errno.EIO:
                                chunk = b''
                            else:
                                raise
                        if not chunk:
                            self.fail(f'Process ended before {expected!r}: {data.decode(errors="replace")}')
                        data += chunk
                consumed = data.index(marker, consumed) + len(marker)
                if callable(response):
                    response = response()
                # Let hidden-input mode activate before submitting a credential.
                if 'hidden' in expected:
                    time.sleep(.05)
                os.write(master, response.encode())
            while time.monotonic() < deadline:
                if select.select([master], [], [], .1)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    data += chunk
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    pid = None
                    break
            while pid is not None and time.monotonic() < deadline:
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    pid = None
                    break
                time.sleep(.01)
            if pid is not None:
                self.fail('Terminal command did not exit')
            return os.waitstatus_to_exitcode(status), data.decode(errors='replace')
        finally:
            if pid is not None:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            os.close(master)

    def test_enter_defaults_save_local_settings_without_creating_vault(self):
        code, output = self.interact(['setup'], [
            ('Use recommended local settings', '\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        cfg = json.loads(self.config.read_text())
        model_val = cfg.get('openai_model') or cfg.get('reranker')
        self.assertEqual((cfg['backend'], model_val, cfg['entity_extraction']), ('onnx', 'minilm-squad2', 'off'))
        self.assertFalse(self.db.exists())
        self.assertFalse((self.config.parent / 'models').exists())

    def test_declining_setup_saves_nothing(self):
        code, output = self.interact(['setup'], [
            ('Use recommended local settings', '\n'),
            ('Save these shared settings', 'n\n'),
        ])
        self.assertEqual(code, 0, output)
        self.assertFalse(self.config.exists())
        self.assertFalse(self.db.exists())

    def test_config_answer_prompt_applies_reviewed_value(self):
        self.demo()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '2\n'), ('Answer engine', 'minilm-squad2\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        cfg = json.loads(self.config.read_text())
        model_val = cfg.get('openai_model') or cfg.get('reranker')
        self.assertEqual(model_val, 'minilm-squad2')
        self.assertFalse(self.db.exists())

    def test_config_answer_prompt_takes_recommended_default_on_enter(self):
        self.demo()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '2\n'), ('Answer engine', '\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        cfg = json.loads(self.config.read_text())
        model_val = cfg.get('openai_model') or cfg.get('reranker')
        self.assertEqual(model_val, 'minilm-squad2')
        self.assertFalse(self.db.exists())

    def test_config_search_prompt_takes_recommended_default_on_enter(self):
        self.demo()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '1\n'), ('Search embedder', '\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        cfg = json.loads(self.config.read_text())
        self.assertEqual(cfg.get('backend'), 'onnx')
        self.assertEqual(cfg.get('embedding_model'), 'bge-small')
        self.assertFalse(self.db.exists())

    def test_config_submenu_back_to_main_menu(self):
        self.demo()
        before = self.config.read_bytes()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '2\n'), ('Answer engine', '0\n'),
            ('Choose [1-5]', '0\n'),
        ])
        self.assertEqual(code, 0, output)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertFalse(self.db.exists())

    def test_cancel_and_invalid_choice_preserve_existing_config(self):
        self.demo()
        before = self.config.read_bytes()
        for steps in [
            [('Choose [1-5]', '0\n')],
            [('Choose [1-5]', '77\n')],
            [('Choose [1-5]', '2\n'), ('Answer engine', 'minilm-squad2\n'), ('Save these shared settings', 'n\n')],
        ]:
            self.interact(['config', '--edit'], steps)
            self.assertEqual(self.config.read_bytes(), before)
            self.assertFalse(self.db.exists())

    def test_hidden_credential_never_appears_in_preview_or_receipt(self):
        self.demo()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '5\n'), ('Credential:', '1\n'),
            ('Action:', 'replace\n'), ('(hidden):', 'terminal-test-secret\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        self.assertNotIn('terminal-test-secret', output)
        cfg = json.loads(self.config.read_text())
        self.assertEqual(cfg.get('openai_api_key') or cfg.get('ollama_api_key'), 'terminal-test-secret')

    def test_concurrent_settings_change_is_not_overwritten(self):
        self.demo()
        def change_elsewhere():
            cfg = json.loads(self.config.read_text())
            cfg['embedding_model'] = 'changed-elsewhere'
            self.config.write_text(json.dumps(cfg))
            return 'y\n'
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '2\n'), ('Answer engine', 'minilm-squad2\n'),
            ('Save these shared settings', change_elsewhere),
        ])
        self.assertNotEqual(code, 0, output)
        cfg = json.loads(self.config.read_text())
        self.assertEqual(cfg['embedding_model'], 'changed-elsewhere')
        model_val = cfg.get('openai_model') or cfg.get('reranker')
        self.assertEqual(model_val, 'top1')

    def test_eof_in_prompt_does_not_apply_default(self):
        code, output = self.interact(['setup'], [('Use recommended local settings', '\x04')])
        self.assertNotEqual(code, 0, output)
        self.assertFalse(self.config.exists())

    def test_setup_and_submenu_retry_on_invalid_choice(self):
        code, output = self.interact(['setup'], [
            ('Use recommended local settings', '99\n'),
            ('Use recommended local settings', '1\n'),
            ('Save these shared settings', 'y\n'),
        ])
        self.assertEqual(code, 0, output)
        self.assertIn('Enter 0-11', output)

        before = self.config.read_bytes()
        code, output = self.interact(['config', '--edit'], [
            ('Choose [1-5]', '1\n'),
            ('Search embedder', '99\n'),
            ('Search embedder', '0\n'),
            ('Choose [1-5]', '0\n'),
        ])
        self.assertEqual(code, 0, output)
        self.assertIn('Choose 0-6', output)
        self.assertEqual(self.config.read_bytes(), before)

    def test_observe_prompt_writes_to_stderr_not_stdout(self):
        import pty
        self.demo()
        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [str(BINARY), '--db', str(self.db), 'observe', 'User: I prefer dark mode', '--confidence', '0.5', '--reranker', 'top1'],
            env=self.env,
            cwd=self.root,
            stdin=slave,
            stdout=subprocess.PIPE,
            stderr=slave,
        )
        os.close(slave)
        stderr_data = b''
        deadline = time.monotonic() + 15
        try:
            while b'Save this memory?' not in stderr_data:
                if time.monotonic() >= deadline:
                    self.fail(f'Timeout waiting for observe prompt: {stderr_data.decode(errors="replace")}')
                if select.select([master], [], [], .1)[0]:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        break
                    stderr_data += chunk
            os.write(master, b'n\n')
            stdout_data, _ = proc.communicate(timeout=10)
            while select.select([master], [], [], .05)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                stderr_data += chunk
        finally:
            os.close(master)
        self.assertEqual(proc.returncode, 0)
        self.assertNotIn('Save this memory?', stdout_data.decode(errors='replace'))
        self.assertIn('Save this memory?', stderr_data.decode(errors='replace'))


if __name__ == '__main__':
    unittest.main()
