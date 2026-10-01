"""LightMem Python Client.

Provides a fast, typed interface to the LightMem Rust engine.
"""

import json
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any, Dict, List, Optional, Union

from .models import AnswerResult, MemoryRecord, MemoryStatus, MemoryType, ScoredMemory, StorageStats


class LightMemError(Exception):
    """Raised when LightMem CLI returns an error."""
    pass


class LightMem:
    """LightMem agent memory client."""

    def __init__(
        self,
        db_path: Optional[Union[str, Path]] = None,
        global_db: bool = False,
        lmem_binary: Optional[str] = None,
    ):
        """Initialize the LightMem client.

        Args:
            db_path: Path to custom SQLite database file. If omitted, uses local project .lightmem.db.
            global_db: If True, forces use of global user database (~/.lightmem/memories.db).
            lmem_binary: Optional path to lmem binary. Defaults to searching PATH and ~/.cargo/bin/lmem.
        """
        self.db_path = str(db_path) if db_path else None
        self.global_db = global_db
        self.binary = self._resolve_binary(lmem_binary)

    def _resolve_binary(self, custom_path: Optional[str]) -> str:
        if custom_path and os.path.exists(custom_path):
            return custom_path

        found = shutil.which("lmem")
        if found:
            return found

        cargo_bin = os.path.expanduser("~/.cargo/bin/lmem")
        if os.path.exists(cargo_bin):
            return cargo_bin

        return "lmem"

    def _run_cmd(self, args: List[str]) -> str:
        cmd = [self.binary]
        if self.db_path:
            cmd.extend(["--db", self.db_path])
        if self.global_db:
            cmd.append("-g")

        cmd.extend(args)

        env = os.environ.copy()
        if self.db_path:
            env["LIGHTMEM_DB"] = self.db_path

        res = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            env=env,
        )

        if res.returncode != 0:
            err = res.stderr.strip() or res.stdout.strip()
            raise LightMemError(f"Command {' '.join(cmd)} failed (code {res.returncode}): {err}")

        return res.stdout.strip()

    def remember(
        self,
        content: str,
        category: Optional[Union[MemoryType, str]] = None,
        title: Optional[str] = None,
        tags: Optional[List[str]] = None,
        confidence: float = 0.9,
    ) -> MemoryRecord:
        """Store a new memory into the database.

        Args:
            content: The text content of the memory.
            category: MemoryType or string (fact, decision, instruction, preference, etc.)
            title: Short title (defaults to first line of content)
            tags: List of string tags
            confidence: Confidence score between 0.0 and 1.0

        Returns:
            MemoryRecord: The stored memory record.
        """
        args = ["remember", content, "--confidence", str(confidence), "--json"]
        if category:
            cat_str = category.value if isinstance(category, MemoryType) else str(category)
            args.extend(["--type", cat_str])
        if title:
            args.extend(["--title", title])
        if tags:
            args.extend(["--tags", ",".join(tags)])

        raw = self._run_cmd(args)
        data = json.loads(raw)
        return MemoryRecord.from_dict(data)

    def recall(
        self,
        query: str,
        category: Optional[Union[MemoryType, str]] = None,
        as_of: Optional[str] = None,
        limit: int = 10,
        min_similarity: Optional[float] = None,
    ) -> List[ScoredMemory]:
        """Perform hybrid BM25 + Vector semantic recall.

        Args:
            query: Natural language query string.
            category: Optional category filter.
            as_of: Historical point-in-time date (YYYY-MM-DD or RFC3339).
            limit: Maximum candidate memories to return.
            min_similarity: Minimum cosine similarity threshold (0.0 - 1.0).

        Returns:
            List[ScoredMemory]: Ranked memories with RRF / hybrid scores.
        """
        args = ["recall", query, "--limit", str(limit), "--json"]
        if category:
            cat_str = category.value if isinstance(category, MemoryType) else str(category)
            args.extend(["--type", cat_str])
        if as_of:
            args.extend(["--as-of", as_of])
        if min_similarity is not None:
            args.extend(["--min-similarity", str(min_similarity)])

        raw = self._run_cmd(args)
        if not raw:
            return []
        data = json.loads(raw)
        return [ScoredMemory.from_dict(item) for item in data]

    def answer(
        self,
        question: str,
        needle: bool = False,
        category: Optional[Union[MemoryType, str]] = None,
        as_of: Optional[str] = None,
        limit: int = 5,
    ) -> AnswerResult:
        """Ask a natural language question and receive a synthesized answer.

        Args:
            question: Natural language question.
            needle: If True, invokes Needle 3 Action SLM for precision slot extraction and card disambiguation.
            category: Optional category filter.
            as_of: Optional historical point-in-time date.
            limit: Max candidate memories to retrieve before reranking.

        Returns:
            AnswerResult: Extracted answer, selected memory card, confidence, and reranker engine used.
        """
        args = ["answer", question, "--limit", str(limit), "--json"]
        if needle:
            args.append("--needle")
        if category:
            cat_str = category.value if isinstance(category, MemoryType) else str(category)
            args.extend(["--type", cat_str])
        if as_of:
            args.extend(["--as-of", as_of])

        raw = self._run_cmd(args)
        data = json.loads(raw)
        return AnswerResult.from_dict(data)

    def list(
        self,
        category: Optional[Union[MemoryType, str]] = None,
        status: Union[MemoryStatus, str] = MemoryStatus.ACTIVE,
        as_of: Optional[str] = None,
        limit: int = 20,
    ) -> List[MemoryRecord]:
        """List memories chronologically.

        Args:
            category: Optional category filter.
            status: "active" or "expired".
            as_of: Historical point-in-time date.
            limit: Maximum memories to return.

        Returns:
            List[MemoryRecord]: Retrieved memory records.
        """
        st_str = status.value if isinstance(status, MemoryStatus) else str(status)
        args = ["list", "--status", st_str, "--limit", str(limit), "--json"]
        if category:
            cat_str = category.value if isinstance(category, MemoryType) else str(category)
            args.extend(["--type", cat_str])
        if as_of:
            args.extend(["--as-of", as_of])

        raw = self._run_cmd(args)
        if not raw:
            return []
        data = json.loads(raw)
        return [MemoryRecord.from_dict(item) for item in data]

    def forget(self, memory_id: str, hard: bool = False) -> bool:
        """Forget or retire a memory by ID.

        Args:
            memory_id: The UUID of the memory.
            hard: If True, permanently deletes from DB. If False (default), soft-expires preserving historical audit trail.

        Returns:
            bool: True if memory was successfully found and retired/deleted.
        """
        args = ["forget", memory_id, "--json"]
        if hard:
            args.append("--hard")

        raw = self._run_cmd(args)
        data = json.loads(raw)
        return bool(data.get("success", False))

    def stats(self) -> StorageStats:
        """Get database storage statistics."""
        raw = self._run_cmd(["stats", "--json"])
        data = json.loads(raw)
        return StorageStats.from_dict(data)

    def export_okf(self, output_path: Optional[Union[str, Path]] = None) -> str:
        """Export memories to an Open Knowledge Format (OKF) markdown bundle."""
        args = ["export", "--okf"]
        if output_path:
            args.extend(["-o", str(output_path)])
        output = self._run_cmd(args)
        # Parse output path from stdout: "✔ Exported OKF bundle to: <path>"
        for line in output.splitlines():
            if "to:" in line:
                return line.split("to:", 1)[1].strip()
        return output

    def import_file(self, file_path: Union[str, Path]) -> int:
        """Import memories from an external file (.json or .okf / .md)."""
        output = self._run_cmd(["import", str(file_path)])
        # Parse count from stdout: "✔ Successfully imported X memories"
        for word in output.split():
            if word.isdigit():
                return int(word)
        return 0
