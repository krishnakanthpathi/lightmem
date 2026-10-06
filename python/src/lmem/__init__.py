"""
LightMem (lmem): Ultra-Fast Local AI Agent Persistent Memory Engine
with Neural Extractive QA & Knowledge Graph.
"""

from .client import LightMem
from .models import (
    Memory,
    ScoredMemory,
    AnswerResult,
    RelatedMemory,
    StorageStats,
    PaginatedList,
    MemoryInspection,
)
from .binary import get_binary_path

__version__ = "0.2.5"
__all__ = [
    "LightMem",
    "Memory",
    "ScoredMemory",
    "AnswerResult",
    "RelatedMemory",
    "StorageStats",
    "PaginatedList",
    "MemoryInspection",
    "get_binary_path",
]
