# LightMem (`lmem`) Python SDK

**Ultra-Fast Local AI Agent Persistent Memory Engine with Neural Extractive QA, Multimodal BLOB Vault & Knowledge Graph.**

[![PyPI version](https://img.shields.io/pypi/v/lmem.svg)](https://pypi.org/project/lmem/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

LightMem combines **SQLite WAL + FTS5 BM25** with **lazy-initialized ONNX vector search** (`fastembed`), **local ONNX Extractive QA (`minilm-squad2` / `tinyroberta-squad2`) & swappable Ollama rerankers** (`ollama:<model>`), **immutable SHA-256-verified multimodal BLOB attachments** (images, audio, video, documents), **Obsidian-grade `[[wikilinks]]` & multi-hop knowledge graph traversal**, **nearest-neighbor conflict merging**, and **temporal point-in-time (`as_of`) & single-day (`date`) queries** — with zero Python dependencies.

---

## ⚡ Quickstart

### Installation
```bash
pip install lmem
```

### Python SDK Usage

```python
from lmem import LightMem

# Initialize vault (defaults to ~/.lightmem/memories.db or custom db_path)
lm = LightMem()

# 1. Store a memory (auto-infers across 14 categories if omitted)
memory = lm.remember(
    "PostgreSQL primary runs on port 5432 with WAL archiving enabled",
    category="fact",
    title="PostgreSQL Primary Config",
    tags=["postgres", "database", "production"],
    confidence=0.95,
)
print("Stored memory:", memory.id)

# 2. Store Multimodal Attachments (images, audio, 4K video, PDFs in isolated 0600 SQLite BLOB vault)
diagram = lm.remember(
    "API Gateway routes to [[PostgreSQL Primary Config]]",
    files=["./ingress-architecture.png"],
    tags=["architecture", "gateway"],
    extract=False,  # True to invoke configured LLM for visual/document summary extraction
)

# 3. Hybrid Search (FTS5 BM25 + Vector Cosine RRF + Graph Multi-Hop & Media Filter)
results = lm.recall("postgres port", limit=5, multi_hop=True)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# Materialize verified attachment file paths directly on disk
image_paths = lm.recall("gateway", media="image", limit=1, path=True)
inspected_paths = lm.inspect(diagram.id, path=True)

# 4. Neural Extractive QA (exact fact extraction via ONNX SQuAD-2.0 or Ollama)
answer = lm.answer("what port does postgresql use?")
print("Extracted answer:", answer.answer)      # "5432"
print("Confidence:", answer.confidence)
print("Reranker:", answer.reranker_used)

# 5. Conversational Observation & Atomic Fact Extraction
candidates = lm.observe(
    "User: We switched Redis to port 6380.\nAssistant: Updated!",
    dry_run=True,
)
for c in candidates:
    print(f"[{c.category}] {c.title}: {c.content}")

# 6. Intentional Knowledge Graph Linking & Multi-Hop Traversal
lm.link(diagram.id, memory.id, relation="depends_on", weight=1.0)
related = lm.related(memory.id, hops=2)
for rel in related:
    print(f"Hop {rel.distance}: {rel.memory.title} via {' -> '.join(rel.relation_path)}")

# 7. Conflict Detection, Deduplication & Vault Telemetry
conflicts = lm.conflicts(min_similarity=0.80)
stats = lm.stats()
print(f"Active memories: {stats.active_memories} | Vectors: {stats.vector_count}")

# 8. Deploy Skill to AI Agents (Antigravity, Codex, Hermes, Cursor, Claude)
lm.connect("antigravity")
```

---

## 🧩 Python SDK Reference (`LightMem`)

| Method | Description |
| :--- | :--- |
| `lm.remember(content, ..., files=None, extract=False, supersede=False)` | Store a memory and optional binary attachments (`files=[...]`), auto-linking `[[wikilinks]]`. |
| `lm.recall(query, limit=5, category=None, media=None, multi_hop=False, path=False)` | Hybrid BM25 + Cosine RRF search; returns `List[ScoredMemory]` or `List[str]` file paths when `path=True`. |
| `lm.answer(question, category=None, reranker=None, as_of=None, date=None)` | Extractive QA via local ONNX (`minilm-squad2`, `tinyroberta-squad2`), `ollama:<model>`, or `top1`. |
| `lm.observe(input_text_or_path, yes=False, dry_run=False, reranker=None)` | Extract atomic candidate memories from dialogue transcripts or notes. |
| `lm.inspect(memory_id, *extra_ids, save_to=None, path=False)` | Inspect full record + graph links + attachments, or materialize verified file paths (`path=True`). |
| `lm.list(page=None, limit=20, category=None, media=None, path=False)` | Paginated chronological listing (`PaginatedList` or `List[str]` paths when `path=True`). |
| `lm.link(source, target, relation="relates_to", weight=1.0)` | Create or update a directional knowledge graph edge. |
| `lm.unlink(source, target, relation=None)` | Remove a directional edge between two memories. |
| `lm.related(memory_id, hops=2)` | Breadth-first multi-hop traversal with hop-decay scoring. |
| `lm.graph(focus=None, hops=2)` | Export full or neighborhood knowledge graph `{nodes, edges}`. |
| `lm.conflicts(yes=False, min_similarity=0.78, reranker=None)` | Detect (and optionally auto-merge with `yes=True`) conflicting or near-duplicate memories. |
| `lm.deduplicate()` | Smart-merge exact duplicate memories. |
| `lm.forget(memory_id, hard=False)` | Soft-expire (`hard=False`) or permanently purge (`hard=True`, cascading BLOBs & cached files). |
| `lm.stats()` | Vault telemetry (`StorageStats`: active/expired counts, vector count, categories, DB size). |
| `lm.connect(platform=None, workspace=False, path=None)` | Install `SKILL.md` to AI coding agents (`antigravity`, `codex`, `hermes`, `cursor`, `claude`, `all`). |

---

## 💻 CLI Commands (Included)

The `pip install lmem` package also installs the standalone `lmem` (and `lightmem`) CLI:

```bash
lmem remember "Redis cache runs on port 6379" -t fact
lmem remember "Itachi 4K wallpaper" -f ./itachi-4k.mp4 --extract
lmem observe conversation.txt --yes
lmem recall "redis port" -l 5 --multi-hop
lmem recall "itachi" -m video -l 1 --path
lmem inspect <ID> --paths
lmem answer "what is my pan card no"
lmem link <SRC_ID> <DST_ID> -r "depends_on"
lmem related <ID> --hops 2
lmem graph --focus <ID> --hops 2
lmem conflicts
lmem connect all
```

---

## 📄 License
MIT License. Created by Pathi Krishna Kanth.

