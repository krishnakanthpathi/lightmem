"""
Comprehensive Functionality & Integrity test suite for the lmem Python SDK.
Uses an isolated temporary database and local workspace binary to verify all operations.
"""
import os
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PYTHON_SRC = REPO_ROOT / "python" / "src"
if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))

DEBUG_BIN = REPO_ROOT / "target" / "debug" / ("lmem.exe" if os.name == "nt" else "lmem")
RELEASE_BIN = REPO_ROOT / "target" / "release" / ("lmem.exe" if os.name == "nt" else "lmem")
if "LMEM_BINARY_PATH" not in os.environ:
    if DEBUG_BIN.is_file():
        os.environ["LMEM_BINARY_PATH"] = str(DEBUG_BIN)
    elif RELEASE_BIN.is_file():
        os.environ["LMEM_BINARY_PATH"] = str(RELEASE_BIN)

import lmem
import lightmem
from lmem.binary import _is_native_binary


class TestLightMemSDK(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.db_path = Path(self.temp_dir.name) / "test_vault.db"
        self.client = lmem.LightMem(db_path=self.db_path)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_package_exports(self):
        self.assertEqual(lmem.__version__, "0.4.0")
        self.assertEqual(lightmem.__version__, "0.4.0")
        self.assertTrue(callable(lmem.LightMem))
        self.assertTrue(callable(lightmem.LightMem))
        self.assertTrue(Path(self.client.binary_path).is_file())

    def test_stats_empty(self):
        stats = self.client.stats()
        self.assertEqual(stats.total_memories, 0)
        self.assertEqual(stats.active_memories, 0)

    def test_remember_get_and_list(self):
        # 1. Remember
        mem = self.client.remember(
            content="Redis sentinel cluster runs on port 26379",
            category="fact",
            title="Redis Sentinel Port",
            tags=["redis", "cache", "sentinel"],
            confidence=0.95,
            provenance="observed",
        )
        self.assertIsNotNone(mem.id)
        self.assertEqual(mem.category, "fact")
        self.assertEqual(mem.title, "Redis Sentinel Port")
        self.assertEqual(mem.provenance, "observed")
        self.assertIn("redis", mem.tags)

        # 2. Get
        fetched = self.client.get(mem.id)
        self.assertIsNotNone(fetched)
        self.assertEqual(fetched.id, mem.id)
        self.assertEqual(fetched.content, mem.content)
        self.assertEqual(fetched.provenance, "observed")

        # 3. List
        paginated = self.client.list(limit=10)
        self.assertEqual(len(paginated), 1)
        self.assertEqual(paginated.total, 1)
        self.assertEqual(paginated[0].id, mem.id)
        self.assertEqual(paginated[0].provenance, "observed")
        self.assertEqual(paginated.items[0].id, mem.id)

    def test_recall_and_answer(self):
        self.client.remember(
            content="PostgreSQL primary database operates on port 5432",
            category="fact",
            tags=["postgres", "database"],
        )
        self.client.remember(
            content="Redis caching server operates on port 6379",
            category="fact",
            tags=["redis", "cache"],
        )

        # Recall
        results = self.client.recall("postgres", limit=5)
        self.assertGreaterEqual(len(results), 1)
        self.assertIn("5432", results[0].memory.content)
        self.assertGreater(results[0].score, 0.0)

        # Answer
        ans = self.client.answer("What port does PostgreSQL operate on?", reranker="top1")
        self.assertIsNotNone(ans.answer)
        self.assertIn("5432", ans.answer)
        self.assertIsNotNone(ans.evidence)

    def test_linking_and_graph(self):
        m1 = self.client.remember(content="Auth Service runs on port 8000")
        m2 = self.client.remember(content="User DB Postgres runs on port 5432")

        # Link
        src_id, dst_id = self.client.link(m1.id, m2.id, relation="depends_on")
        self.assertTrue(src_id)
        self.assertTrue(dst_id)

        # Related
        related = self.client.related(m1.id, hops=1)
        self.assertEqual(len(related), 1)
        self.assertEqual(related[0].memory.id, m2.id)

        # Graph & Autolink
        graph = self.client.graph()
        self.assertIn("nodes", graph)
        self.assertIn("edges", graph)
        self.assertEqual(len(graph["nodes"]), 2)
        self.assertEqual(len(graph["edges"]), 1)

        autolink_res = self.client.autolink(min_similarity=0.75)
        self.assertIn("total_nodes", autolink_res)

        # Unlink
        unlinked = self.client.unlink(m1.id, m2.id, relation="depends_on")
        self.assertTrue(unlinked)

    def test_inspect_and_deduplicate(self):
        m1 = self.client.remember(content="Unique microservice endpoint", title="Microservice A")
        m2 = self.client.remember(content="Service dependency node", title="Service B")
        self.client.link(m1.id, m2.id, relation="calls")

        # Inspect
        inspection = self.client.inspect(m1.id)
        self.assertIsNotNone(inspection)
        self.assertEqual(inspection.memory.id, m1.id)
        self.assertEqual(len(inspection.links), 1)
        self.assertEqual(inspection.links[0]["relation"], "calls")

        # Deduplicate on clean vault
        merged = self.client.deduplicate()
        self.assertEqual(merged, 0)

    def test_forget_and_deduplicate(self):
        m = self.client.remember(content="Temporary staging token is secret123")
        self.assertEqual(self.client.stats().total_memories, 1)

        # Forget
        forgotten = self.client.forget(m.id, hard=True)
        self.assertTrue(forgotten)
        self.assertEqual(self.client.stats().total_memories, 0)

    def test_observe_conversational_ingestion(self):
        chat = "User: I prefer dark mode and JetBrains Mono.\nAssistant: Noted!"

        # 1. Dry run returns ObservedCandidate instances without saving
        candidates = self.client.observe(chat, dry_run=True, reranker="top1")
        self.assertGreaterEqual(len(candidates), 1)
        self.assertIsInstance(candidates[0], lmem.ObservedCandidate)
        self.assertIn("dark mode", candidates[0].content.lower())
        self.assertEqual(self.client.stats().total_memories, 0)

        # 2. Persisting with yes=True returns saved Memory instances
        saved = self.client.observe(chat, yes=True, reranker="top1")
        self.assertGreaterEqual(len(saved), 1)
        self.assertIsInstance(saved[0], lmem.Memory)
        self.assertEqual(saved[0].provenance, "observed")
        self.assertEqual(self.client.stats().total_memories, len(saved))

    def test_attachments_and_paths(self):
        import struct

        png_path = Path(self.temp_dir.name) / "diagram.png"
        png_bytes = (
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
            + struct.pack(">II", 320, 240)
            + b"\x08\x06\x00\x00\x00"
            + b"X" * 32
        )
        png_path.write_bytes(png_bytes)

        # 1. File-only remember via SDK
        mem = self.client.remember(files=[png_path], tags=["diagram", "ui"])
        self.assertEqual(mem.provenance, "attachment_metadata")
        self.assertEqual(len(mem.attachments), 1)
        self.assertEqual(mem.attachments[0].filename, "diagram.png")
        self.assertEqual(mem.attachments[0].media_type, "image")
        self.assertEqual(mem.attachments[0].media_kind, "image")
        self.assertEqual(mem.attachments[0].dimensions, "320x240")

        # Verify stats() schema fields (vector_count, categories, storage_size_bytes, db_path)
        st = self.client.stats()
        self.assertEqual(st.total_memories, 1)
        self.assertEqual(st.vector_count, 1)
        self.assertEqual(st.total_vectors, 1)
        self.assertEqual(st.categories.get("artifact"), 1)
        self.assertGreater(st.storage_size_bytes, 0)
        self.assertTrue(st.db_path.endswith("test_vault.db"))

        # 2. Inspect with path=True returns materialized path list
        paths = self.client.inspect(mem.id, path=True)
        self.assertEqual(len(paths), 1)
        self.assertTrue(Path(paths[0]).exists())
        self.assertEqual(Path(paths[0]).read_bytes(), png_bytes)

        # 3. Recall with media filter and path=True
        rec_paths = self.client.recall("diagram", media="image", path=True)
        self.assertEqual(len(rec_paths), 1)
        self.assertEqual(Path(rec_paths[0]).read_bytes(), png_bytes)

        # 4. List with media filter and path=True
        list_paths = self.client.list(media="image", path=True)
        self.assertEqual(len(list_paths), 1)

    def test_func_connect_and_conflicts(self):
        skill_dir = Path(self.temp_dir.name) / "custom_skill"
        deployed = self.client.connect(path=str(skill_dir))
        self.assertEqual(len(deployed), 1)
        self.assertTrue((skill_dir / "lightmem" / "SKILL.md").is_file())

        conflicts = self.client.conflicts(min_similarity=0.8, reranker="top1")
        self.assertIsInstance(conflicts, list)

    def test_integrity_binary_validation_and_error_handling(self):
        # 1. _is_native_binary must reject shell scripts starting with #!
        fake_script = Path(self.temp_dir.name) / "fake_lmem.sh"
        fake_script.write_bytes(b"#!/bin/sh\necho 'fake'\n")
        fake_script.chmod(0o755)
        self.assertFalse(_is_native_binary(fake_script))
        self.assertTrue(_is_native_binary(self.client.binary_path))

        # 2. Blank remember without files raises RuntimeError
        with self.assertRaises(RuntimeError):
            self.client.remember(content="   ")

        # 3. Invalid confidence raises RuntimeError
        with self.assertRaises(RuntimeError):
            self.client.remember(content="Valid text", confidence=2.5)

        # 4. Missing ID returns None on get/inspect and [] on inspect(path=True)
        self.assertIsNone(self.client.get("nonexistent-id-0000"))
        self.assertIsNone(self.client.inspect("nonexistent-id-0000"))
        self.assertEqual(self.client.inspect("nonexistent-id-0000", path=True), [])


if __name__ == "__main__":
    unittest.main()
