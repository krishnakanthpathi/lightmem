"""LightMem: Ultra-fast native agent memory engine."""

from typing import Any, Dict, List, Optional

class MemoryRecord:
    id: str
    category: str
    title: str
    content: str
    tags: List[str]
    confidence: float
    status: str
    provenance: str
    created_at: str
    updated_at: str
    expired_at: Optional[str]

    def to_dict(self) -> Dict[str, Any]: ...

class ScoredMemory:
    memory: MemoryRecord
    score: float

    def to_dict(self) -> Dict[str, Any]: ...

class AnswerResult:
    answer: str
    selected_memory: Optional[MemoryRecord]
    confidence: float
    reranker_used: str

    def to_dict(self) -> Dict[str, Any]: ...

class StorageStats:
    total_memories: int
    active_memories: int
    expired_memories: int
    total_vectors: int
    by_category: Dict[str, int]

    def to_dict(self) -> Dict[str, Any]: ...

class LightMem:
    """LightMem agent memory engine client."""

    def __init__(self, db_path: Optional[str] = None, global_db: bool = False) -> None: ...
    @property
    def db_path(self) -> str: ...
    def remember(
        self,
        content: str,
        category: Optional[str] = None,
        title: Optional[str] = None,
        tags: Optional[List[str]] = None,
        confidence: Optional[float] = None,
    ) -> MemoryRecord: ...
    def recall(
        self,
        query: str,
        category: Optional[str] = None,
        as_of: Optional[str] = None,
        limit: Optional[int] = None,
        min_similarity: Optional[float] = None,
    ) -> List[ScoredMemory]: ...
    def answer(
        self,
        question: str,
        needle: Optional[bool] = None,
        category: Optional[str] = None,
        as_of: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> AnswerResult: ...
    def list(
        self,
        category: Optional[str] = None,
        status: Optional[str] = None,
        as_of: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> List[MemoryRecord]: ...
    def forget(self, id: str, hard: bool = False) -> bool: ...
    def stats(self) -> StorageStats: ...
    def export_okf(self, output_path: Optional[str] = None) -> str: ...
    def import_file(self, file_path: str) -> int: ...

__version__: str
