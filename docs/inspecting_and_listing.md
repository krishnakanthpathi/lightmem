# ❖ Inspecting & Listing Guide (`lmem inspect` & `lmem list`)

Command-driven reference for browsing, paginating, and deeply inspecting memories, attachments, and knowledge graph links in LightMem.

---

## 1. Syntax

```bash
# List memories chronologically with pagination
lmem list [OPTIONS]

# Inspect memory records by ID, prefix, or title (aliases: get, show)
lmem inspect [OPTIONS] <TERMS>...
```

---

## 2. Chronological Listing (`lmem list`)

### Browse Recent Memories
```bash
lmem list
```
* Displays active memories in reverse chronological order.
* Default: 20 records per page.

### Pagination Controls (`--limit`, `--page`, `--offset`)
```bash
# Return first 5 records
lmem list -l 5

# Jump directly to page 2 (1-indexed)
lmem list -l 5 --page 2

# Skip 10 records and return next 5 (0-indexed offset)
lmem list -l 5 --offset 10
```

---

## 3. Status Filtering (`--status`)

### Active Memories (Default)
```bash
lmem list --status active
```

### Expired & Superseded Memories
```bash
lmem list --status expired
```
* Lists memories that have soft-expired via `--ttl` or were superseded by `--supersede`.

---

## 4. Category & Media Filtering

### Filter Listed Memories by Category (`-t, --type`)
```bash
# List decisions only
lmem list -t decision

# List instructions only
lmem list -t instruction

# List artifacts/files only
lmem list -t artifact
```

### Filter by Media Type (`-m, --media`)
```bash
# List memories with image attachments
lmem list -m image

# List memories with code attachments
lmem list -m code

# List memories with video attachments
lmem list -m video
```

Supported kinds: `image`, `audio`, `video`, `document`, `code`, `archive`, `binary`, `any`.

---

## 5. Deep Memory Inspection (`lmem inspect`)

### Inspect by UUID or Prefix
```bash
# Full UUID
lmem inspect 4156db6c-e16f-4d04-bae9-034a8c8d426d

# Short 8-character prefix
lmem inspect 4156db6c
```

Output:
```
◈ Postgres Port Update (4156db6c) [fact] ● active
  Vault: ~/.lightmem/memories.db

  PostgreSQL 16 runs on port 5433 with WAL enabled and SSL

  ├─ Tags:        #db #postgres
  ├─ Confidence:  0.90
  ├─ Provenance:  explicit_statement
  ├─ Created:     2026-10-30T14:36:21.316048+00:00
  ╰─ Updated:     2026-10-30T14:36:23.096052+00:00

  ◫ Knowledge Graph Links (1):
    ├─ ──references──▶ (77ccf460) [weight: 1.0]
```

### Inspect by Title
```bash
lmem inspect "Postgres Port Update"
```

### Batch Inspection
```bash
# Inspect multiple memories at once
lmem inspect 4156db6c 6d2c0d11 77ccf460
```

---

## 6. Attachment Materialization & Export

### Print Absolute File Paths to stdout (`-p, --path`)
```bash
# Print verified path of single memory attachment
lmem inspect 6d2c0d11 -p

# Print verified paths of all listed code attachments
lmem list -m code -p
```

Output:
```
/tmp/.lightmem_attachments/sample_schema.sql
```

### Shell Automation & Piping
```bash
# View attachment contents
cat $(lmem inspect 6d2c0d11 -p)

# Open image attachment in system viewer
open $(lmem inspect 1ba8baf1 -p)
```

### Export Attachments to Disk (`-o, --save-to`)
```bash
# Export single memory attachments
lmem inspect 6d2c0d11 -o ./exports/

# Export all attachments from listed memories
lmem list -m image -o ./exported_images/
```

---

## 7. Temporal Listing (`--as-of` vs `--date`)

### Point-in-Time Cumulative View (`--as-of`)
```bash
# View active state as of an exact date
lmem list --as-of 2026-09-15
lmem list --as-of 2026-09-15T12:00:00Z
```

### Calendar Day Filter (`--date`)
```bash
# Memories created or updated on that specific UTC day
lmem list --date 2026-10-30
```

---

## 8. Multi-Tenant Scopes & Connections

```bash
# List memories inside a named connection vault
lmem --connection agy list

# Inspect memory inside a specific connection
lmem --connection agy inspect 4156db6c

# List memories inside an agent-isolated scope
lmem --connection agy --scope agent list
```

---

## 9. Vault Overrides

```bash
# Target global database
lmem -g list

# Target custom database file directly
lmem --db /path/to/custom.db inspect 4156db6c
```

---

## 10. Machine-Readable JSON Output (`--json`)

### Paginated Listing Schema (`lmem list --json`)
```bash
lmem list -l 2 --page 1 --json
```

Output:
```json
{
  "items": [
    {
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
        { "name": "port", "val": "5433" }
      ]
    }
  ],
  "total": 8,
  "limit": 2,
  "offset": 0,
  "page": 1,
  "total_pages": 4,
  "has_more": true
}
```

### Deep Inspection Schema (`lmem inspect --json`)
```bash
lmem inspect 4156db6c --json
```

Output:
```json
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
      { "name": "port", "val": "5433" }
    ]
  },
  "links": [
    {
      "target_id": "77ccf460-1011-4022-b912-302910293021",
      "relation": "references",
      "weight": 1.0
    }
  ]
}
```

---

## 11. Python SDK Equivalents

```python
from lmem import LightMem

lm = LightMem()

# 1. Paginated list
page = lm.list(limit=10, page=1, category="fact")
print(f"Total: {page.total}, Pages: {page.total_pages}")
for mem in page.items:
    print(f"- [{mem.category}] {mem.title} ({mem.id})")

# 2. Inspect single memory
mem = lm.inspect("4156db6c")
print(f"Title: {mem.title}")
print(f"Tags: {mem.tags}")
print(f"Entities: {[e.val for e in mem.entities]}")

# 3. Materialize attachment path
file_paths = lm.inspect("6d2c0d11", path=True)
print(f"Attachment on disk: {file_paths}")

# 4. List expired memories
expired = lm.list(status="expired")
```
