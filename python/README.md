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

# 8. Single-File DB & Media Transfer Bundles (Zero Re-embedding)
# Packages checkpointed SQLite DB, all attached videos/photos/docs & vector embeddings into a .zip
lm.bundle_export("my-vault.zip")
info = lm.bundle_info("my-vault.zip")
print(f"Backed up {info['record_count']} memories & {info['attachment_count']} media files")
lm.bundle_import("my-vault.zip", mode="replace")  # or mode="merge"

# 9. Deploy Skill to AI Agents (Antigravity, Codex, Hermes, Cursor, Claude)
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
| `lm.bundle_export(output_path)` | Export complete vault into a portable compressed `.zip` bundle (DB, vectors & all video/photo files). |
| `lm.bundle_import(bundle_path, mode="replace")` | Restore (`mode="replace"`) or merge (`mode="merge"`) vault from a `.zip` transfer bundle. |
| `lm.bundle_info(bundle_path)` | Inspect bundle manifest (record counts, attachments, schema) without extracting. |
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
lmem bundle export -o backup.zip
lmem bundle info backup.zip
lmem bundle import backup.zip --merge
lmem connect all
```

---

## Single-File DB & Media Transfer Bundles (v0.5.0)

Transfer complete memory vaults between machines, servers, or workstations in a single compressed file—including all SQLite tables, full vector embeddings, knowledge graph links, and attached physical video, photo, audio, and PDF files:

```bash
# Export complete vault with clean SQLite checkpoint (VACUUM INTO) & all physical media
lmem bundle export -o my-vault.zip
lmem export --bundle -o my-vault.zip

# Inspect archive contents, record counts, and attachments without unzipping
lmem bundle info my-vault.zip

# Restore on a new machine with zero re-indexing or re-embedding overhead
lmem bundle import my-vault.zip              # Clean restore (default: replace)
lmem bundle import my-vault.zip --merge      # Merge into existing memories
```

Bundles use the standard `.zip` (or `.lmbundle`) format containing:
- `memories.db`: Pristine SQLite database snapshot generated via `VACUUM INTO` (WAL flushed, defragmented).
- `manifest.json`: Verification manifest with schema version, record counts, SHA-256 digests, and media metadata.
- `attachments/`: Physical media files (4K videos, photos, voice notes, PDFs) extracted automatically to the target vault.

---

## 📄 License
MIT License. Created by Pathi Krishna Kanth.



## Identity-aware connections (v0.4.0)

Register the person, the logical agent, and a named harness connection once:

```bash
lmem identity user-add alice --name "Alice"
lmem identity agent-add coding --user alice
lmem identity connection-add agy --user alice --agent coding --harness antigravity --project myapp --memory-scope project
lmem identity connection-add codex --user alice --agent coding --harness codex --project myapp --memory-scope project

lmem --connection agy --session chat-123 remember "Decision: use SQLite WAL" -t decision
lmem --connection codex recall "database decision"
lmem --connection codex context --json
lmem --connection codex --scope agent remember "Current debugging checkpoint"
lmem --connection agy connect
lmem --connection agy connect --mcp
```

Connections for the same user/project open the same persistent project vault across harnesses. `user` scope follows the user; `agent` scope belongs to the user/agent and, when provided, project. Each command uses one scope. Switch with `--scope`; recall does not blend scopes. `LIGHTMEM_CONNECTION` can select a default connection. Do not combine a connection with `--db`, `LIGHTMEM_DB`, or `--global`.

Register custom harnesses and deploy their bound instructions:

```bash
lmem identity harness-add my-harness --path /absolute/path/to/skills
lmem identity connection-add custom --user alice --agent coding --harness my-harness --memory-scope user
lmem --connection custom connect
lmem identity list connections --json
lmem identity list users --json
```

Custom MCP clients can launch `lmem --connection custom mcp`; automatic MCP configuration supports Antigravity, Claude, and Cursor. The MCP `context` tool shows the active binding. Connection IDs, user IDs, agent IDs, project IDs, and optional session IDs accept 1–128 ASCII letters, digits, underscores, or hyphens.

The registry is stored in `~/.lightmem/identities.db` and scoped vaults under `~/.lightmem/vaults/` (or `LIGHTMEM_CONFIG_DIR`). Scope keys use hashes to avoid filesystem case collisions. All memory operations, including graph traversal, deduplication, conflicts, attachments, and export, operate on the selected vault. New memories expose optional `context` source metadata in JSON; exact duplicates retain their original source. Backups preserve source metadata even when deliberately imported into another vault.

Existing commands and databases continue to work. Schema v5 adds optional source context; existing memories are not attributed to a user automatically. To copy existing memories into a new scope, explicitly export the old database and import into the selected connection. These are local organizational identities, not authentication. Connections do not automatically capture chats; agents must invoke `remember` or `observe`.

```python
from lmem import LightMem

admin = LightMem()
admin.register_user("alice")
admin.register_agent("coding", "alice")
admin.register_connection("agy", "alice", "coding", "antigravity", project_id="myapp", scope="project")

memory = LightMem(connection="agy", session="chat-123")
saved = memory.remember("Decision: use SQLite WAL", category="decision")
print(saved.context)
print(memory.context())
```

Python also exposes `register_harness`, `identities`, `connection_info`, and `connect(mcp=True)`. Use different connection IDs for multiple harnesses while retaining the same user/project to share decisions.

