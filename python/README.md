# LightMem (`lmem`) Python SDK

Ultra-Fast Local AI Agent Persistent Memory Engine with Neural Extractive QA & Knowledge Graph.

[![PyPI version](https://img.shields.io/pypi/v/lmem.svg)](https://pypi.org/project/lmem/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

---

## ⚡ Quickstart

### Installation
```bash
pip install lmem
```

### Python SDK Usage

```python
from lmem import LightMem

# Initialize vault (defaults to ~/.lightmem/memories.db or custom path)
lm = LightMem()

# 1. Store a memory (auto-infers 14 categories)
memory = lm.remember(
    "PostgreSQL primary runs on port 5432 with WAL archiving enabled",
    category="fact",
    title="PostgreSQL Primary Config",
    tags=["postgres", "database", "production"],
    confidence=0.95,
)
print("Stored memory:", memory.id)

# 2. Hybrid Search (FTS5 BM25 + Vector Cosine RRF)
results = lm.recall("postgres port", limit=5)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# 3. Neural Extractive QA (exact fact extraction in ~6ms via ONNX SQuAD2)
answer = lm.answer("what port does postgresql use?")
print("Extracted answer:", answer.answer)      # "5432"
print("Confidence:", answer.confidence)
print("Reranker:", answer.reranker_used)

# 4. Knowledge Graph Autolink (title mentions, shared tags, vector similarity)
link_stats = lm.autolink(min_similarity=0.75)
print("Connections created:", link_stats)

# 5. Multi-Hop Graph Traversal
related = lm.related(memory.id, hops=2)
for rel in related:
    print(f"Hop {rel.distance}: {rel.memory.title} via {' -> '.join(rel.relation_path)}")

# 6. Deploy Skill to AI Agents (Antigravity, Codex, Hermes, Cursor, Claude)
lm.connect("antigravity")
```

---

## 💻 CLI Commands (Included)

The `pip install lmem` package also installs the standalone CLI:

```bash
lmem remember "Redis cache runs on port 6379" -t fact
lmem recall "redis port" -l 5
lmem answer "what is my pan card no"
lmem autolink
lmem graph
lmem connect all
```

---

## 📄 License
MIT License. Created by Pathi Krishna Kanth.
