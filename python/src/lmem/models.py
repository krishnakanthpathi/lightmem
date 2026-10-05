from __future__ import annotations
from dataclasses import dataclass, field
from typing import List, Optional, Any, Dict


@dataclass
class Memory:
    id: str
    category: str
    title: str
    content: str
    tags: List[str] = field(default_factory=list)
    confidence: float = 0.9
    status: str = "active"
    provenance: str = "explicit_statement"
    created_at: str = ""
    updated_at: str = ""
    expired_at: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Memory:
        mem_data = data.get("memory", data)
        return cls(
            id=mem_data.get("id", ""),
            category=mem_data.get("category", "fact"),
            title=mem_data.get("title", ""),
            content=mem_data.get("content", ""),
            tags=mem_data.get("tags", []),
            confidence=float(mem_data.get("confidence", 0.9)),
            status=mem_data.get("status", "active"),
            provenance=mem_data.get("provenance", "explicit_statement"),
            created_at=mem_data.get("created_at", ""),
            updated_at=mem_data.get("updated_at", ""),
            expired_at=mem_data.get("expired_at"),
        )


@dataclass
class MemoryInspection:
    memory: Memory
    links: List[Dict[str, Any]]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> MemoryInspection:
        return cls(
            memory=Memory.from_dict(data.get("memory", data)),
            links=data.get("links", []),
        )


@dataclass
class ScoredMemory:
    memory: Memory
    score: float
    vector_rank: Optional[int] = None
    bm25_rank: Optional[int] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ScoredMemory:
        mem_data = data.get("memory", data)
        return cls(
            memory=Memory.from_dict(mem_data),
            score=float(data.get("score", 0.0)),
            vector_rank=data.get("vector_rank"),
            bm25_rank=data.get("bm25_rank"),
        )


@dataclass
class AnswerResult:
    answer: str
    confidence: float
    reranker_used: str
    selected_memory: Optional[Memory] = None

    @property
    def evidence(self) -> Optional[str]:
        return self.selected_memory.content if self.selected_memory else None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> AnswerResult:
        selected = data.get("selected_memory")
        return cls(
            answer=data.get("answer", ""),
            confidence=float(data.get("confidence", 0.0)),
            reranker_used=data.get("reranker_used", ""),
            selected_memory=Memory.from_dict(selected) if selected else None,
        )


@dataclass
class RelatedMemory:
    memory: Memory
    distance: int
    relation_path: List[str]
    score: float

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> RelatedMemory:
        mem_data = data.get("memory", {})
        return cls(
            memory=Memory.from_dict(mem_data),
            distance=int(data.get("distance", 1)),
            relation_path=data.get("relation_path", []),
            score=float(data.get("score", 1.0)),
        )


@dataclass
class StorageStats:
    total_memories: int
    active_memories: int
    expired_memories: int
    vector_count: int
    categories: Dict[str, int]
    storage_size_bytes: int
    db_path: str
    backend: Optional[str] = None
    embedding_model: Optional[str] = None
    reranker: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> StorageStats:
        return cls(
            total_memories=int(data.get("total_memories", 0)),
            active_memories=int(data.get("active_memories", 0)),
            expired_memories=int(data.get("expired_memories", 0)),
            vector_count=int(data.get("vector_count", 0)),
            categories=data.get("categories", {}),
            storage_size_bytes=int(data.get("storage_size_bytes", 0)),
            db_path=data.get("db_path", ""),
            backend=data.get("backend"),
            embedding_model=data.get("embedding_model"),
            reranker=data.get("reranker"),
        )


class PaginatedList(list):
    """A list of items that also carries pagination metadata (total, page, etc.)."""
    def __init__(
        self,
        items: List[Memory],
        total: int = 0,
        limit: int = 20,
        offset: int = 0,
        page: int = 1,
        total_pages: int = 1,
        has_more: bool = False,
    ):
        super().__init__(items)
        self.total = total
        self.limit = limit
        self.offset = offset
        self.page = page
        self.total_pages = total_pages
        self.has_more = has_more

    @property
    def items(self) -> List[Memory]:
        return list(self)
