"""LightMem Standalone Python Module.

Single-file drop-in client for LightMem agent memory.
Zero third-party dependencies (uses Python standard library only).
"""

from dataclasses import dataclass, field
from enum import Enum
import json
import os
from pathlib import Path
import shutil
import subprocess
from typing import Any, Dict, List, Optional, Union


class MemoryType(str, Enum):
    FACT = "fact"
    DECISION = "decision"
    INSTRUCTION = "instruction"
    PREFERENCE = "preference"
    LEARNING = "learning"
    GOAL = "goal"
    COMMITMENT = "commitment"
    ARTIFACT = "artifact"
    EVENT = "event"
    ERROR = "error"
    STATE = "state"
    RELATIONSHIP = "relationship"
    OBSERVATION = "observation"


class MemoryStatus(str, Enum):
    ACTIVE = "active"
    EXPIRED = "expired"


@dataclass
class MemoryRecord:
    id: str
    category: str
    title: str
    content: str
    tags: List[str] = field(default_factory=list)
    confidence: float = 0.9
    status: str = "active"
    provenance: Optional[str] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
    expired_at: Optional[str] = None

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "MemoryRecord":
        return cls(
            id=d.get("id", ""),
            category=d.get("category", "fact"),
            title=d.get("title", ""),
            content=d.get("content", ""),
            tags=d.get("tags") or [],
            confidence=float(d.get("confidence", 0.9)),
            status=d.get("status", "active"),
            provenance=d.get("provenance"),
            created_at=d.get("created_at"),
            updated_at=d.get("updated_at"),
            expired_at=d.get("expired_at"),
        )


@dataclass
class ScoredMemory:
    memory: MemoryRecord
    score: float

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "ScoredMemory":
        mem = MemoryRecord.from_dict(d.get("memory", {}))
        return cls(
            memory=mem,
            score=float(d.get("score", 0.0)),
        )


@dataclass
class AnswerResult:
    answer: str
    selected_memory: Optional[MemoryRecord]
    confidence: float
    reranker_used: str

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "AnswerResult":
        selected = (
            MemoryRecord.from_dict(d["selected_memory"])
            if d.get("selected_memory")
            else None
        )
        return cls(
            answer=d.get("answer", ""),
            selected_memory=selected,
            confidence=float(d.get("confidence", 0.0)),
            reranker_used=d.get("reranker_used", "top1"),
        )


@dataclass
class StorageStats:
    total_memories: int
    active_memories: int
    expired_memories: int
    total_vectors: int
    by_category: Dict[str, int]

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "StorageStats":
        by_cat_raw = d.get("by_category", [])
        by_cat = {}
        if isinstance(by_cat_raw, list):
            for item in by_cat_raw:
                if isinstance(item, (list, tuple)) and len(item) == 2:
                    by_cat[str(item[0])] = int(item[1])
        elif isinstance(by_cat_raw, dict):
            by_cat = {str(k): int(v) for k, v in by_cat_raw.items()}

        return cls(
            total_memories=int(d.get("total_memories", 0)),
            active_memories=int(d.get("active_memories", 0)),
            expired_memories=int(d.get("expired_memories", 0)),
            total_vectors=int(d.get("total_vectors", 0)),
            by_category=by_cat,
        )


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
        args = ["forget", memory_id, "--json"]
        if hard:
            args.append("--hard")

        raw = self._run_cmd(args)
        data = json.loads(raw)
        return bool(data.get("success", False))

    def stats(self) -> StorageStats:
        raw = self._run_cmd(["stats", "--json"])
        data = json.loads(raw)
        return StorageStats.from_dict(data)

    def export_okf(self, output_path: Optional[Union[str, Path]] = None) -> str:
        args = ["export", "--okf"]
        if output_path:
            args.extend(["-o", str(output_path)])
        output = self._run_cmd(args)
        for line in output.splitlines():
            if "to:" in line:
                return line.split("to:", 1)[1].strip()
        return output

    def import_file(self, file_path: Union[str, Path]) -> int:
        output = self._run_cmd(["import", str(file_path)])
        for word in output.split():
            if word.isdigit():
                return int(word)
        return 0
