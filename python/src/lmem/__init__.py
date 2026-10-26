"""
LightMem (lmem): Ultra-Fast Local AI Agent Persistent Memory Engine
with Neural Extractive QA & Knowledge Graph.
"""

from .client import LightMem
from .models import (
    AttachmentMetadata,
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

__version__ = "0.5.0"
__all__ = [
    "LightMem",
    "AttachmentMetadata",
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

