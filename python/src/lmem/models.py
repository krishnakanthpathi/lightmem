from __future__ import annotations
from dataclasses import dataclass, field
from typing import List, Optional, Any, Dict


@dataclass
class AttachmentMetadata:
    id: str
    memory_id: str
    filename: str
    mime_type: str
    media_type: str
    size_bytes: int
    sha256: str
    dimensions: Optional[str] = None
    duration_secs: Optional[float] = None
    created_at: str = ""

    @property
    def media_kind(self) -> str:
        return self.media_type

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> AttachmentMetadata:
        dur = data.get("duration_secs")
        return cls(
            id=data.get("id", ""),
            memory_id=data.get("memory_id", ""),
            filename=data.get("filename", ""),
            mime_type=data.get("mime_type", "application/octet-stream"),
            media_type=data.get("media_type") or data.get("media_kind", "binary"),
            size_bytes=int(data.get("size_bytes", 0)),
            sha256=data.get("sha256", ""),
            dimensions=data.get("dimensions"),
            duration_secs=float(dur) if dur is not None else None,
            created_at=data.get("created_at", ""),
        )


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
    attachments: List[AttachmentMetadata] = field(default_factory=list)
    saved_attachments: List[str] = field(default_factory=list)
    context: Optional[Dict[str, Any]] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Memory:
        mem_data = data.get("memory", data)
        raw_atts = mem_data.get("attachments", [])
        saved_atts = mem_data.get("saved_attachments", data.get("saved_attachments", []))
        return cls(
            context=mem_data.get("context"),
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
            attachments=[AttachmentMetadata.from_dict(a) for a in raw_atts],
            saved_attachments=[str(p) for p in saved_atts],
        )


@dataclass
class MemoryInspection:
    memory: Memory
    links: List[Dict[str, Any]]
    saved_attachments: List[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> MemoryInspection:
        saved = data.get("saved_attachments", [])
        mem = Memory.from_dict(data.get("memory", data))
        if saved and not mem.saved_attachments:
            mem.saved_attachments = [str(p) for p in saved]
        return cls(
            memory=mem,
            links=data.get("links", []),
            saved_attachments=[str(p) for p in saved],
        )


@dataclass
class ScoredMemory:
    memory: Memory
    score: float
    vector_rank: Optional[int] = None
    bm25_rank: Optional[int] = None
    saved_attachments: List[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ScoredMemory:
        mem_data = data.get("memory", data)
        saved = data.get("saved_attachments", [])
        mem = Memory.from_dict(mem_data)
        if saved and not mem.saved_attachments:
            mem.saved_attachments = [str(p) for p in saved]
        return cls(
            memory=mem,
            score=float(data.get("score", 0.0)),
            vector_rank=data.get("vector_rank"),
            bm25_rank=data.get("bm25_rank"),
            saved_attachments=[str(p) for p in saved],
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

    @property
    def total_vectors(self) -> int:
        return self.vector_count

    @property
    def by_category(self) -> List[ Any ]:
        return list(self.categories.items())

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> StorageStats:
        vec_cnt = data.get("vector_count")
        if vec_cnt is None:
            vec_cnt = data.get("total_vectors", 0)
        raw_cats = data.get("categories")
        if isinstance(raw_cats, dict):
            cats = {str(k): int(v) for k, v in raw_cats.items()}
        elif isinstance(data.get("by_category"), list):
            cats = {
                str(item[0]): int(item[1])
                for item in data["by_category"]
                if isinstance(item, (list, tuple)) and len(item) >= 2
            }
        else:
            cats = {}
        return cls(
            total_memories=int(data.get("total_memories", 0)),
            active_memories=int(data.get("active_memories", 0)),
            expired_memories=int(data.get("expired_memories", 0)),
            vector_count=int(vec_cnt),
            categories=cats,
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
        saved_attachments: Optional[List[str]] = None,
    ):
        super().__init__(items)
        self.total = total
        self.limit = limit
        self.offset = offset
        self.page = page
        self.total_pages = total_pages
        self.has_more = has_more
        self.saved_attachments = saved_attachments or []

    @property
    def items(self) -> List[Memory]:
        return list(self)


@dataclass
class ObservedCandidate:
    title: str
    content: str
    category: str
    tags: List[str] = field(default_factory=list)
    confidence: float = 0.85
    provenance: str = "observed"

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ObservedCandidate:
        return cls(
            title=data.get("title", ""),
            content=data.get("content", ""),
            category=data.get("category", "observation"),
            tags=data.get("tags", []),
            confidence=float(data.get("confidence", 0.85)),
            provenance=data.get("provenance", "observed"),
        )

