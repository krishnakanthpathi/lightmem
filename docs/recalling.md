# ❖ Recalling Guide (`lmem recall`)

Command-driven reference for hybrid keyword and semantic memory retrieval in LightMem.

---

## 1. Syntax

```bash
lmem recall [OPTIONS] <QUERY>
```

---

## 2. Basic Hybrid Search & Limits

### Search Memories
```bash
lmem recall "postgres port"
```
* Combines SQLite FTS5 BM25 keyword matching with ONNX vector embeddings using Reciprocal Rank Fusion (RRF).
* Defaults to returning top 10 matches.

### Custom Result Limit
```bash
# Return top 3 matches
lmem recall "auth configuration" -l 3

# Return top 20 matches
lmem recall "docker deployment" --limit 20
```

### Minimum Similarity Threshold
```bash
# Only return strong matches (similarity >= 0.80)
lmem recall "production database" --min-similarity 0.80
```
* Range: `0.0` to `1.0`. Filters out low-confidence semantic matches.

---

## 3. Category Filtering (`-t, --type`)

### Restrict Search to a Specific Memory Category
```bash
# Search decisions only
lmem recall "database" -t decision

# Search instructions only
lmem recall "git commit" -t instruction

# Search preferences only
lmem recall "theme" -t preference

# Search artifacts/files only
lmem recall "schema" -t artifact
```

Supported types: `fact`, `decision`, `instruction`, `preference`, `learning`, `goal`, `commitment`, `artifact`, `event`, `relationship`, `observation`, `error`, `context`, `password`.

---

## 4. Media Filtering (`-m, --media`)

### Filter by Attached File Type
```bash
# Images (PNG, JPG, SVG, WebP)
lmem recall "architecture diagram" -m image

# Code files (SQL, Rust, Python, Go, JS)
lmem recall "user schema" -m code

# Videos (MP4, MKV, WebM)
lmem recall "feature demo" -m video

# Audio (MP3, WAV, M4A, OGG)
lmem recall "meeting voice note" -m audio

# Documents (PDF, Markdown, TXT)
lmem recall "quarterly report" -m document

# Any attachment
lmem recall "backup" -m any
```

Supported kinds: `image`, `audio`, `video`, `document`, `code`, `archive`, `binary`, `any`.

---

## 5. Instant Path Materialization (`-p, --path`)

### Print Verified File Paths to stdout
```bash
# Materialize file and print only its absolute path
lmem recall "architecture diagram" -m image -p -l 1
```

Output:
```
/tmp/.lightmem_attachments/architecture.png
```

### Shell Piping & Automation
```bash
# Open image directly
open $(lmem recall "architecture diagram" -m image -p -l 1)

# Inspect SQL file content
cat $(lmem recall "user table schema" -m code -p -l 1)

# Stream video
mpv $(lmem recall "ui demo" -m video -p -l 1)
```

---

## 6. Export Attachments (`-o, --save-to`)

### Save Attached Binaries to Target Directory
```bash
# Save to folder
lmem recall "architecture" -m image -o ./exported/

# Save single file to specific output path
lmem recall "user schema" -m code -o ./exported/schema.sql -l 1
```

---

## 7. Temporal Filtering (`--as-of` vs `--date`)

### Point-in-Time Cumulative Snapshot (`--as-of`)
```bash
# State as of an exact date (end of day UTC)
lmem recall "postgres port" --as-of 2026-09-15

# State as of an exact RFC3339 timestamp
lmem recall "postgres port" --as-of 2026-09-15T14:30:00Z
```
* Returns memories created on or before that timestamp that were active at that moment.
* Includes memories that were later superseded or expired after that date.

### Single-Day Exact Filter (`--date`)
```bash
# Only memories created or updated on that specific UTC calendar day
lmem recall "incident report" --date 2026-10-30
```
* Format: `YYYY-MM-DD`. Restricts results strictly to activity on that calendar day.

---

## 8. Knowledge Graph Expansion (`--multi-hop`)

### Expand Query Hits with Graph Neighbors
```bash
lmem recall "API Gateway" --multi-hop
```
* Retrieves primary search hits matching `"API Gateway"`.
* Expands the result set with 1-hop connected neighbors linked via `[[wikilinks]]` or explicit `lmem link` edges.

---

## 9. Precision QA Mode (`--precision`)

### Inline Fact Extraction
```bash
lmem recall "postgres port" --precision
```
* Runs the Extractive QA / entity index engine over candidate memories.
* Extracts pinpoint answer span (`5433`) directly in the terminal alongside the source memory.

---

## 10. Multi-Tenant Scopes & Connections

### Recall from a Scoped Vault
```bash
# Query within a named connection
lmem --connection agy recall "database architecture"

# Query within an agent-isolated scope
lmem --connection agy --scope agent recall "scratchpad"

# Query within a specific user scope
lmem --connection agy --scope user recall "preferences"
```

---

## 11. Vault Overrides

### Global Vault
```bash
lmem -g recall "editor preference"
```
* Forces query against `~/.lightmem/memories.db`, bypassing local workspace vaults.

### Custom Database Path
```bash
lmem --db /path/to/vault.db recall "deployment"
```

---

## 12. Machine-Readable JSON Output (`--json`)

```bash
lmem recall "postgres" -l 1 --json
```

Output:
```json
[
  {
    "memory": {
      "id": "4156db6c-e16f-4d04-bae9-034a8c8d426d",
      "category": "fact",
      "title": "Postgres Port Update",
      "content": "PostgreSQL 16 runs on port 5433 with WAL enabled and SSL",
      "tags": ["db", "postgres"],
      "confidence": 0.9,
      "status": "active",
      "provenance": "explicit_statement",
      "created_at": "2026-10-30T14:36:21.316048Z",
      "updated_at": "2026-10-30T14:36:23.096052Z",
      "expired_at": null,
      "entities": [
        { "name": "software", "val": "PostgreSQL 16" },
        { "name": "port", "val": "5433" }
      ]
    },
    "score": 0.8688,
    "bm25_rank": 1,
    "vector_rank": 2
  }
]
```

---

## 13. Python SDK Equivalents

```python
from lmem import LightMem

lm = LightMem()

# 1. Basic hybrid recall
results = lm.recall("postgres port", limit=5)
for r in results:
    print(f"[{r.score:.2f}] {r.memory.title}: {r.memory.content}")

# 2. Filter by category
decisions = lm.recall("database", category="decision")

# 3. Filter by media and get absolute file paths
image_paths = lm.recall("architecture", media="image", path=True)

# 4. Temporal point-in-time recall
historical = lm.recall("postgres port", as_of="2026-09-15")

# 5. Multi-hop knowledge graph expansion
expanded = lm.recall("API Gateway", multi_hop=True)
```
