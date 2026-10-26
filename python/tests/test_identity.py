"""Exercise identity registration and scoped memory through the public Python SDK."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

REPO_ROOT = Path(__file__).resolve().parents[2]
PYTHON_SRC = REPO_ROOT / "python" / "src"
if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))

from lmem import LightMem
from lmem.binary import ensure_binary_downloaded


class IdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.env = patch.dict(os.environ, {"LIGHTMEM_CONFIG_DIR": str(self.root), "LIGHTMEM_OFFLINE": "1"})
        self.env.start()
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(self.env.stop)
        (self.root / "config.json").write_text(json.dumps({
            "backend": "hash", "onnx_model": None, "ollama_url": "http://localhost:11434",
            "embedding_model": "test", "reranker": "top1",
        }))
        admin = LightMem()
        admin.register_user("alice", "Alice")
        admin.register_agent("builder", "alice")
        admin.register_agent("reviewer", "alice")
        admin.register_connection("agy", "alice", "builder", "antigravity", "app", "project")
        admin.register_connection("codex", "alice", "reviewer", "codex", "app", "project")
        self.admin = admin

    def test_cross_harness_sharing_source_and_private_scope(self):
        agy = LightMem(connection="agy", session="chat-42")
        codex = LightMem(connection="codex")
        memory = agy.remember("Decision: persist application data in SQLite WAL", category="decision")
        self.assertEqual(memory.context["session_id"], "chat-42")
        self.assertEqual(codex.get(memory.id).context, memory.context)
        self.assertEqual(codex.recall("SQLite WAL")[0].memory.id, memory.id)
        private = LightMem(connection="agy", scope="agent")
        note = private.remember("Private builder checkpoint")
        self.assertIsNone(LightMem(connection="codex", scope="agent").get(note.id))
        self.assertEqual(codex.stats().total_memories, 1)
        self.assertEqual(agy.context()["db_path"], codex.context()["db_path"])
        self.assertEqual(self.admin.connection_info("agy")["user_id"], "alice")
        self.assertEqual(len(self.admin.identities()), 2)

    def test_custom_harness_binding(self):
        target = self.root / "custom-skills"
        self.admin.register_harness("custom", target, "My harness")
        self.admin.register_connection("custom-link", "alice", "builder", "custom")
        result = LightMem(connection="custom-link").connect()
        self.assertEqual(len(result), 1)
        skill = (target / "lightmem" / "SKILL.md").read_text()
        self.assertIn("lmem --connection custom-link --scope user remember", skill)

    def test_download_failure_does_not_fetch_an_incompatible_old_release(self):
        with patch("lmem.binary.Path.home", return_value=self.root), patch("lmem.binary.urllib.request.urlretrieve", side_effect=OSError("unavailable")) as download:
            with self.assertRaises(RuntimeError):
                ensure_binary_downloaded()
            self.assertEqual(download.call_count, 1)
            self.assertIn("v0.4.0", download.call_args.args[0])

    def test_invalid_identity_and_ambiguous_options(self):
        with self.assertRaises(RuntimeError):
            self.admin.register_user("../escape")
        with self.assertRaises(RuntimeError):
            self.admin.register_agent("missing-owner", "nobody")
        with self.assertRaises(RuntimeError):
            self.admin.register_connection("missing-project", "alice", "builder", "codex", scope="project")
        for kwargs in ({"connection": "agy", "db_path": "other.db"},
                       {"connection": "agy", "global_db": True}, {"scope": "user"},
                       {"connection": "agy", "scope": "invalid"}):
            with self.assertRaises(ValueError):
                LightMem(**kwargs)


if __name__ == "__main__":
    unittest.main()
