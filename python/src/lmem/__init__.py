"""
LightMem (lmem): Ultra-Fast Local AI Agent Persistent Memory Engine
with Neural Extractive QA & Knowledge Graph.
"""

from .client import LightMem
from .models import (
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
)
from .binary import get_binary_path

__version__ = "0.7.7"
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
]

