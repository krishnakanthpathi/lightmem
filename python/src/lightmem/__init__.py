"""
LightMem alias package re-exporting lmem.
"""

from lmem import (
    LightMem,
    AttachmentMetadata,
    EntityRecord,
    Memory,
    ScoredMemory,
    AnswerResult,
    RelatedMemory,
    StorageStats,
    PaginatedList,
    MemoryInspection,
    ObservedCandidate,
    get_binary_path,
    __version__,
)

__all__ = [
    "LightMem",
    "AttachmentMetadata",
    "EntityRecord",
    "Memory",
    "ScoredMemory",
    "AnswerResult",
    "RelatedMemory",
    "StorageStats",
    "PaginatedList",
    "MemoryInspection",
    "ObservedCandidate",
    "get_binary_path",
    "__version__",
]
