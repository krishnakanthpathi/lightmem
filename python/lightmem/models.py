"""Data models and enums for LightMem Python client."""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional


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
