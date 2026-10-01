"""LightMem: Ultra-fast, lightweight, cross-platform agent memory engine."""

from .client import LightMem, LightMemError
from .models import (
    AnswerResult,
    MemoryRecord,
    MemoryStatus,
    MemoryType,
    ScoredMemory,
    StorageStats,
)

__version__ = "0.1.0"
__all__ = [
    "LightMem",
    "LightMemError",
    "MemoryRecord",
    "MemoryStatus",
    "MemoryType",
    "ScoredMemory",
    "StorageStats",
    "AnswerResult",
]
