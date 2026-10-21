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
    ObservedCandidate,
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
        *,
        connection: Optional[str] = None,
        scope: Optional[str] = None,
        session: Optional[str] = None,
    ):
        if connection and (db_path or global_db):
            raise ValueError("connection cannot be combined with db_path or global_db")
        if (scope or session) and not connection:
            raise ValueError("scope and session require a connection")
        if scope is not None and scope not in ("user", "project", "agent"):
            raise ValueError("scope must be user, project, or agent")
        self.connection = connection
        self.scope = scope
        self.session = session
        self.db_path = str(db_path) if db_path else None
        self.global_db = global_db

    @property
    def binary_path(self) -> str:
        return get_binary_path()

    def _build_base_args(self) -> List[str]:
        args: List[str] = []
        if self.connection:
            args.extend(["--connection", self.connection])
            if self.scope:
                args.extend(["--scope", self.scope])
            if self.session:
                args.extend(["--session", self.session])
        elif self.global_db:
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

    def context(self) -> Dict[str, Any]:
        """Inspect the active identity, scope, and persistent vault path."""
        return self._call("context", [])

    def register_user(self, user_id: str, name: Optional[str] = None) -> Dict[str, Any]:
        args = ["user-add", user_id]
        if name is not None:
            args.extend(["--name", name])
        return self._call("identity", args)

    def register_agent(self, agent_id: str, user_id: str, name: Optional[str] = None) -> Dict[str, Any]:
        args = ["agent-add", agent_id, "--user", user_id]
        if name is not None:
            args.extend(["--name", name])
        return self._call("identity", args)

    def register_harness(self, harness_id: str, path: Optional[str | Path] = None, name: Optional[str] = None) -> Dict[str, Any]:
        args = ["harness-add", harness_id]
        if path is not None:
            args.extend(["--path", str(path)])
        if name is not None:
            args.extend(["--name", name])
        return self._call("identity", args)

    def register_connection(self, connection_id: str, user_id: str, agent_id: str, harness_id: str,
                            project_id: Optional[str] = None, scope: str = "user") -> Dict[str, Any]:
        args = ["connection-add", connection_id, "--user", user_id, "--agent", agent_id,
                "--harness", harness_id, "--memory-scope", scope]
        if project_id is not None:
            args.extend(["--project", project_id])
        return self._call("identity", args)

    def identities(self, kind: str = "connections") -> List[Dict[str, Any]]:
        return self._call("identity", ["list", kind])

    def connection_info(self, connection_id: str) -> Dict[str, Any]:
        return self._call("identity", ["show", connection_id])

    # 1. Store
    def remember(
        self,
        content: Optional[str] = None,
        category: Optional[str] = None,
        title: Optional[str] = None,
        tags: Optional[List[str]] = None,
        confidence: float = 0.9,
        ttl: Optional[str] = None,
        supersede: bool = False,
        provenance: Optional[str] = None,
        files: Optional[List[str | Path]] = None,
        extract: bool = False,
        reranker: Optional[str] = None,
    ) -> Memory:
        args: List[str] = []
        if content is not None and (content or not files):
            args.append(content)
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
        if files:
            for f in files:
                args.extend(["-f", str(f)])
        if extract:
            args.append("--extract")
        if reranker:
            args.extend(["-r", reranker])

        data = self._call("remember", args)
        return Memory.from_dict(data)

    # 2. Hybrid Recall
    def recall(
        self,
        query: str,
        limit: int = 5,
        category: Optional[str] = None,
        media: Optional[str] = None,
        min_similarity: Optional[float] = None,
        multi_hop: bool = False,
        as_of: Optional[str] = None,
        date: Optional[str] = None,
        save_to: Optional[str | Path] = None,
        path: bool = False,
    ) -> List[ScoredMemory] | List[str]:
        args = [query, "-l", str(limit)]
        if category:
            args.extend(["-t", category])
        if media:
            args.extend(["--media", media])
        if min_similarity is not None:
            args.extend(["--min-similarity", str(min_similarity)])
        if multi_hop:
            args.append("--multi-hop")
        if as_of:
            args.extend(["--as-of", as_of])
        if date:
            args.extend(["--date", date])
        if save_to:
            args.extend(["--save-to", str(save_to)])
        if path:
            args.append("--path")

        data = self._call("recall", args)
        if path:
            return [str(p) for p in data] if isinstance(data, list) else []
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
        media: Optional[str] = None,
        status: str = "active",
        as_of: Optional[str] = None,
        date: Optional[str] = None,
        save_to: Optional[str | Path] = None,
        path: bool = False,
    ) -> PaginatedList | List[str]:
        args = ["-l", str(limit), "--status", status]
        if page is not None:
            args.extend(["--page", str(page)])
        elif offset > 0:
            args.extend(["--offset", str(offset)])
        if category:
            args.extend(["-t", category])
        if media:
            args.extend(["--media", media])
        if as_of:
            args.extend(["--as-of", as_of])
        if date:
            args.extend(["--date", date])
        if save_to:
            args.extend(["--save-to", str(save_to)])
        if path:
            args.append("--path")

        data = self._call("list", args)
        if path:
            return [str(p) for p in data] if isinstance(data, list) else []
        items = [Memory.from_dict(item) for item in data.get("items", [])]
        return PaginatedList(
            items=items,
            total=int(data.get("total", len(items))),
            limit=int(data.get("limit", limit)),
            offset=int(data.get("offset", offset)),
            page=int(data.get("page", page or 1)),
            total_pages=int(data.get("total_pages", 1)),
            has_more=bool(data.get("has_more", False)),
            saved_attachments=[str(p) for p in data.get("saved_attachments", [])],
        )

    # 5. Get Single Memory
    def get(self, memory_id: str) -> Optional[Memory]:
        try:
            data = self._call("get", [memory_id])
            return Memory.from_dict(data) if data else None
        except RuntimeError:
            return None

    # 5b. Inspect Memory & Graph Links
    def inspect(
        self,
        memory_id: str | List[str],
        *extra_ids: str,
        save_to: Optional[str | Path] = None,
        path: bool = False,
    ) -> Optional[MemoryInspection] | List[MemoryInspection] | List[str]:
        terms: List[str] = []
        if isinstance(memory_id, (list, tuple)):
            terms.extend(str(t) for t in memory_id)
        else:
            terms.append(str(memory_id))
        terms.extend(str(t) for t in extra_ids)

        args = list(terms)
        if save_to:
            args.extend(["--save-to", str(save_to)])
        if path:
            args.append("--path")

        try:
            data = self._call("inspect", args)
            if path:
                return [str(p) for p in data] if isinstance(data, list) else []
            if isinstance(data, list):
                return [MemoryInspection.from_dict(item) for item in data]
            return MemoryInspection.from_dict(data) if data else None
        except RuntimeError:
            return [] if (path or len(terms) > 1) else None

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
        mcp: bool = False,
    ) -> List[Dict[str, Any]]:
        args = []
        if platform:
            args.append(platform)
        if workspace:
            args.append("--workspace")
        if path:
            args.extend(["--path", path])
        if mcp:
            args.append("--mcp")
        return self._call("connect", args)

    # 14. Vault Telemetry & Stats
    def stats(self) -> StorageStats:
        data = self._call("stats", [])
        return StorageStats.from_dict(data)

    # 15. Conversational Observation & Ingestion
    def observe(
        self,
        input_text_or_path: str,
        yes: bool = False,
        dry_run: bool = False,
        category: Optional[str] = None,
        tags: Optional[List[str]] = None,
        confidence: Optional[float] = None,
        reranker: Optional[str] = None,
    ) -> List[Any]:
        args = [input_text_or_path]
        if yes:
            args.append("--yes")
        if dry_run:
            args.append("--dry-run")
        if category:
            args.extend(["-t", category])
        if tags:
            args.extend(["--tags", ",".join(tags)])
        if confidence is not None:
            args.extend(["-c", str(confidence)])
        if reranker:
            args.extend(["-r", reranker])

        data = self._call("observe", args)
        if isinstance(data, list):
            if yes:
                return [Memory.from_dict(item) for item in data]
            else:
                return [ObservedCandidate.from_dict(item) for item in data]
        return data
