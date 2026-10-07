from __future__ import annotations
import json
from typing import List, Optional, Dict, Any, Tuple
from pathlib import Path

from .binary import execute_lmem, get_binary_path
from .models import (
    Memory,
    ScoredMemory,
    AnswerResult,
    RelatedMemory,
    StorageStats,
    PaginatedList,
    MemoryInspection,
)


class LightMem:
    """
    LightMem: Ultra-Fast Local AI Agent Persistent Memory Engine.
    Provides Pythonic methods for remember, recall, answer, graph, and maintenance.
    """

    def __init__(
        self,
        db_path: Optional[str | Path] = None,
        global_db: bool = False,
    ):
        self.db_path = str(db_path) if db_path else None
        self.global_db = global_db

    @property
    def binary_path(self) -> str:
        return get_binary_path()

    def _build_base_args(self) -> List[str]:
        args: List[str] = []
        if self.global_db:
            args.append("--global")
        elif self.db_path:
            args.extend(["--db", self.db_path])
        return args

    def _call(self, subcommand: str, extra_args: List[str], input_text: Optional[str] = None) -> Any:
        full_args = self._build_base_args() + [subcommand] + extra_args + ["--json"]
        code, stdout, stderr = execute_lmem(full_args, input_text=input_text)
        if code != 0:
            err = stderr.strip() or stdout.strip()
            raise RuntimeError(f"lmem {subcommand} failed (exit code {code}): {err}")
        try:
            return json.loads(stdout)
        except json.JSONDecodeError as e:
            raise RuntimeError(f"Failed to parse JSON output from lmem {subcommand}: {stdout}") from e

    # 1. Store
    def remember(
        self,
        content: str,
        category: Optional[str] = None,
        title: Optional[str] = None,
        tags: Optional[List[str]] = None,
        confidence: float = 0.9,
        ttl: Optional[str] = None,
        supersede: bool = False,
        provenance: Optional[str] = None,
    ) -> Memory:
        args = [content]
        if category:
            args.extend(["-t", category])
        if title:
            args.extend(["--title", title])
        if tags:
            args.extend(["--tags", ",".join(tags)])
        if confidence is not None:
            args.extend(["--confidence", str(confidence)])
        if ttl:
            args.extend(["--ttl", ttl])
        if supersede:
            args.append("--supersede")
        if provenance:
            args.extend(["--provenance", provenance])

        data = self._call("remember", args)
        return Memory.from_dict(data)

    # 2. Hybrid Recall
    def recall(
        self,
        query: str,
        limit: int = 5,
        category: Optional[str] = None,
        min_similarity: Optional[float] = None,
        multi_hop: bool = False,
        as_of: Optional[str] = None,
        date: Optional[str] = None,
    ) -> List[ScoredMemory]:
        args = [query, "-l", str(limit)]
        if category:
            args.extend(["-t", category])
        if min_similarity is not None:
            args.extend(["--min-similarity", str(min_similarity)])
        if multi_hop:
            args.append("--multi-hop")
        if as_of:
            args.extend(["--as-of", as_of])
        if date:
            args.extend(["--date", date])

        data = self._call("recall", args)
        if isinstance(data, list):
            return [ScoredMemory.from_dict(item) for item in data]
        return []

    # 3. Neural Extractive QA Answer
    def answer(
        self,
        question: str,
        category: Optional[str] = None,
        reranker: Optional[str] = None,
        limit: int = 10,
        as_of: Optional[str] = None,
        date: Optional[str] = None,
    ) -> AnswerResult:
        args = [question, "-l", str(limit)]
        if category:
            args.extend(["-t", category])
        if reranker:
            args.extend(["-r", reranker])
        if as_of:
            args.extend(["--as-of", as_of])
        if date:
            args.extend(["--date", date])

        data = self._call("answer", args)
        return AnswerResult.from_dict(data)

    # 4. List
    def list(
        self,
        page: Optional[int] = None,
        limit: int = 20,
        offset: int = 0,
        category: Optional[str] = None,
        status: str = "active",
        as_of: Optional[str] = None,
        date: Optional[str] = None,
    ) -> PaginatedList:
        args = ["-l", str(limit), "--status", status]
        if page is not None:
            args.extend(["--page", str(page)])
        elif offset > 0:
            args.extend(["--offset", str(offset)])
        if category:
            args.extend(["-t", category])
        if as_of:
            args.extend(["--as-of", as_of])
        if date:
            args.extend(["--date", date])

        data = self._call("list", args)
        items = [Memory.from_dict(item) for item in data.get("items", [])]
        return PaginatedList(
            items=items,
            total=int(data.get("total", len(items))),
            limit=int(data.get("limit", limit)),
            offset=int(data.get("offset", offset)),
            page=int(data.get("page", page or 1)),
            total_pages=int(data.get("total_pages", 1)),
            has_more=bool(data.get("has_more", False)),
        )

    # 5. Get Single Memory
    def get(self, memory_id: str) -> Optional[Memory]:
        try:
            data = self._call("get", [memory_id])
            return Memory.from_dict(data) if data else None
        except RuntimeError:
            return None

    # 5b. Inspect Memory & Graph Links
    def inspect(self, memory_id: str) -> Optional[MemoryInspection]:
        try:
            data = self._call("inspect", [memory_id])
            return MemoryInspection.from_dict(data) if data else None
        except RuntimeError:
            return None

    # 6. Forget (Soft or Hard)
    def forget(self, memory_id: str, hard: bool = False) -> bool:
        args = [memory_id]
        if hard:
            args.append("--hard")
        data = self._call("forget", args)
        return bool(data.get("success", False))

    # 7. Deduplicate & Merge
    def deduplicate(self) -> int:
        data = self._call("dedup", [])
        return int(data.get("merged", 0))

    # 8. Nearest-Neighbor Conflicts
    def conflicts(
        self,
        yes: bool = False,
        min_similarity: float = 0.78,
        reranker: Optional[str] = None,
    ) -> List[Dict[str, Any]]:
        args = ["--min-similarity", str(min_similarity)]
        if yes:
            args.append("--yes")
        if reranker:
            args.extend(["-r", reranker])
        data = self._call("conflicts", args)
        return data if isinstance(data, list) else []

    # 9. Knowledge Graph Links
    def link(
        self,
        source: str,
        target: str,
        relation: str = "relates_to",
        weight: float = 1.0,
    ) -> Tuple[str, str]:
        args = [source, target, "-r", relation, "-w", str(weight)]
        data = self._call("link", args)
        return (data.get("source", source), data.get("target", target))

    def unlink(self, source: str, target: str, relation: Optional[str] = None) -> bool:
        args = [source, target]
        if relation:
            args.extend(["-r", relation])
        data = self._call("unlink", args)
        return bool(data.get("removed", False))

    # 10. Multi-Hop Graph Traversal
    def related(self, memory_id: str, hops: int = 2) -> List[RelatedMemory]:
        data = self._call("related", [memory_id, "-n", str(hops)])
        if isinstance(data, list):
            return [RelatedMemory.from_dict(item) for item in data]
        return []

    # 11. Graph Snapshot
    def graph(self, focus: Optional[str] = None, hops: int = 2) -> Dict[str, Any]:
        args = ["-n", str(hops)]
        if focus:
            args.extend(["-f", focus])
        return self._call("graph", args)

    # 12. Autolink Vault
    def autolink(self, min_similarity: float = 0.75) -> Dict[str, Any]:
        return self._call("autolink", ["--min-similarity", str(min_similarity)])

    # 13. Connect AI Agent Skills
    def connect(
        self,
        platform: Optional[str] = None,
        workspace: bool = False,
        path: Optional[str] = None,
    ) -> List[Dict[str, Any]]:
        args = []
        if platform:
            args.append(platform)
        if workspace:
            args.append("--workspace")
        if path:
            args.extend(["--path", path])
        return self._call("connect", args)

    # 14. Vault Telemetry & Stats
    def stats(self) -> StorageStats:
        data = self._call("stats", [])
        return StorageStats.from_dict(data)
