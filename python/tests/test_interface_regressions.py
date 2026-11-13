"""Interface regressions using the native CLI and an isolated offline vault."""
import json
import os
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from lmem import LightMem


class InterfaceRegressions(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        env = patch.dict(os.environ, {
            "LIGHTMEM_CONFIG_DIR": str(self.root / "config"),
            "LIGHTMEM_OFFLINE": "1", "LIGHTMEM_QA_DISABLE": "1",
        })
        env.start()
        self.addCleanup(env.stop)
        (self.root / "config").mkdir()
        (self.root / "config/config.json").write_text(json.dumps({
            "backend": "hash", "onnx_model": None, "reranker": "top1",
            "ollama_url": "http://localhost:11434", "embedding_model": "test",
        }))
        self.client = LightMem(db_path=self.root / "memory.db")

    def test_leading_dash_query_is_literal_with_options(self):
        self.client.remember("The --help flag prints command usage", category="fact")
        for query in ("--help", "-l", "--json", "--"):
            with self.subTest(query=query):
                self.assertIsInstance(self.client.recall(query, limit=1, category="fact"), list)
                self.assertIsNotNone(self.client.answer(query, limit=1, category="fact", reranker="top1"))


    def test_accessor_database_failures_propagate(self):
        (self.root / "memory.db").write_bytes(b"not a sqlite database")
        for method, args, kwargs in (
            (self.client.get, ("missing",), {}),
            (self.client.inspect, ("missing",), {}),
            (self.client.inspect, (["missing", "other"],), {}),
            (self.client.inspect, ("missing",), {"path": True}),
        ):
            with self.subTest(method=method.__name__, kwargs=kwargs):
                with self.assertRaisesRegex(RuntimeError, "failed"):
                    method(*args, **kwargs)

    def test_accessor_missing_is_not_an_operational_failure(self):
        self.assertIsNone(self.client.get("missing"))
        self.assertIsNone(self.client.inspect("missing"))
        self.assertEqual(self.client.inspect("missing", path=True), [])
        self.assertEqual(self.client.inspect(["missing", "other"]), [])

    def test_accessor_execution_and_decode_failures_propagate(self):
        for result in ((1, "", "database is locked"), (0, "broken JSON", ""), (2, "null", "")):
            for method in (self.client.get, self.client.inspect):
                with self.subTest(result=result, method=method.__name__):
                    with patch("lmem.client.execute_lmem", return_value=result):
                        with self.assertRaises(RuntimeError):
                            method("id")


if __name__ == "__main__":
    unittest.main()
