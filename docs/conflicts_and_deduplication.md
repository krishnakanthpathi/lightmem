# ❖ Conflicts & Deduplication Guide (`lmem conflicts` & `lmem dedup`)

Command-driven reference for detecting contradictions, resolving overlapping facts, and deduplicating records in LightMem.

---

## 1. Syntax

```bash
# Scan and resolve conflicting or overlapping memories
lmem conflicts [OPTIONS]

# Deduplicate identical memories across the vault (alias: dedup)
lmem deduplicate [OPTIONS]
```

---

## 2. Conflict Scanning (`lmem conflicts`)

### Scan Active Memories for Contradictions
```bash
lmem conflicts
```
* Analyzes active memories using nearest-neighbor vector cosine similarity ($\ge 0.78$) combined with token overlap.
* Pairs older memories with newer memories and displays exact UTC timestamps, similarity %, and token overlap %.

Visual Diff Display:
```
  01/02. [fact] PostgreSQL port is changed to 5433 ▰▰▰▰▰▰▰▰▰▰ 95% sim · 100% overlap
     ├─ [OLDER · 2026-10-31 06:07:42 UTC] (abc9c38e) PostgreSQL database port changed to 5433 [merged & retired]
     ╰─ [NEWER · 2026-10-31 06:08:22 UTC] (fabe6b06) PostgreSQL port is changed to 5433 [active]
```

---

## 3. Interactive vs Batch Resolution

### Interactive Terminal Resolution
```bash
lmem conflicts
```
Prompts for each detected conflict pair:
* `[y]es`: Merges pair (prioritizes newer facts, unions tags, retires older record).
* `[n]o`: Skips to next candidate pair without modifying either record.
* `[a]ll`: Merges current and all remaining conflict pairs automatically.
* `[q]uit`: Exits without processing remaining pairs.

### Automated Batch Merge (`-y, --yes`)
```bash
# Merge all detected conflicts without prompting (headless / agent workflows)
lmem conflicts --yes

# Short flag / alias
lmem conflicts -y
lmem conflicts --resolve
```

---

## 4. Similarity Threshold (`--min-similarity`)

### Adjust Conflict Detection Sensitivity
```bash
# Stricter matching (only flag very high similarity >= 0.85)
lmem conflicts --min-similarity 0.85

# Looser matching (catch subtle variations >= 0.70)
lmem conflicts --min-similarity 0.70
```
* Default: `0.78` (range: `0.0` to `1.0`).

---

## 5. Reranker-Guided Fact Synthesis (`-r, --reranker`)

### Synthesize Merged Clauses Using LLM or QA Models
```bash
# Synthesize with configured default Ollama model
lmem conflicts --yes -r ollama

# Synthesize with a specific local LLM
lmem conflicts --yes -r ollama:qwen2.5:3b

# Instant zero-overhead vector rank-1 merge
lmem conflicts --yes -r top1
```
* Merging rules:
  1. Prioritizes newer fact statements over older contradictory clauses.
  2. Preserves non-conflicting clauses from the older memory.
  3. Case-insensitively unions all tags.
  4. Keeps highest confidence score.
  5. Re-embeds the updated memory and marks the older record as `expired`.

---

## 6. Exact Deduplication (`lmem dedup`)

### Write-Time Automatic Deduplication
Storing an exact identical memory automatically merges into the existing record without creating duplicate IDs:
```bash
lmem remember "PostgreSQL port is 5432" --tags "db"
lmem remember "PostgreSQL port is 5432" --tags "backend"
# Updates existing canonical record with tags: #db #backend
```

### Batch Vault Cleanup (`lmem dedup`)
```bash
# Scan and collapse exact duplicate text records across entire database
lmem dedup

# Full command name
lmem deduplicate
```
* Essential after importing external databases, OKF dumps, or bulk JSON migrations.
* Preserves earliest `created_at`, latest `updated_at`, and unions all tags.

---

## 7. Multi-Tenant Scopes & Connections

```bash
# Scan conflicts inside a named connection
lmem --connection agy conflicts

# Force-merge conflicts in an agent-isolated scope
lmem --connection agy --scope agent conflicts --yes

# Batch dedup inside a connection vault
lmem --connection agy dedup
```

---

## 8. Vault Overrides

```bash
# Run conflict scan on global vault (~/.lightmem/memories.db)
lmem -g conflicts

# Target custom database file directly
lmem --db /path/to/custom.db conflicts --yes
lmem --db /path/to/custom.db dedup
```

---

## 9. Machine-Readable JSON Output (`--json`)

### Conflict Report Schema (`lmem conflicts --json`)
```bash
lmem conflicts --json
```

Output:
```json
{
  "count": 1,
  "resolved": false,
  "conflicts": [
    {
      "similarity": 0.9537,
      "overlap_ratio": 1.0,
      "older_memory": {
        "id": "abc9c38e-6337-40c0-8f86-b36b2862e25b",
        "category": "fact",
        "title": "PG Port Changed",
        "content": "PostgreSQL database port changed to 5433",
        "tags": [],
        "confidence": 0.9,
        "status": "active",
        "created_at": "2026-10-31T06:07:42.785558Z"
      },
      "newer_memory": {
        "id": "fabe6b06-99b4-426c-8eb0-704952d94abc",
        "category": "fact",
        "title": "PostgreSQL port is changed to 5433",
        "content": "PostgreSQL port is changed to 5433",
        "tags": [],
        "confidence": 0.9,
        "status": "active",
        "created_at": "2026-10-31T06:08:22.059179Z"
      }
    }
  ]
}
```

### Deduplication Schema (`lmem dedup --json`)
```bash
lmem dedup --json
```

Output:
```json
{
  "duplicates_removed": 2,
  "database": "/Users/user/.lightmem/memories.db"
}
```

---

## 10. Python SDK Equivalents

```python
from lmem import LightMem

lm = LightMem()

# 1. Scan conflicts without resolving
conflicts = lm.conflicts()
print(f"Detected {len(conflicts)} conflict pairs")
for c in conflicts:
    print(f"Match: {c.similarity:.2f} | Older: {c.older_memory.id} vs Newer: {c.newer_memory.id}")

# 2. Batch resolve conflicts with custom threshold
lm.conflicts(resolve=True, min_similarity=0.80)

# 3. Resolve conflicts with specific LLM synthesis
lm.conflicts(resolve=True, reranker="ollama:qwen2.5:3b")

# 4. Batch deduplicate vault
removed_count = lm.deduplicate()
print(f"Removed {removed_count} exact duplicates")
```
