# ❖ Remembering Guide (`lmem remember`)

Command-driven reference for storing facts, decisions, files, and relationships in LightMem.

---

## 1. Syntax

```bash
lmem remember [OPTIONS] [CONTENT]
```

---

## 2. Basic Text & Titles

### Store Plain Memory
```bash
lmem remember "PostgreSQL 16 runs on port 5432 with WAL enabled"
```
* Stores memory content.
* Generates UUID.
* Derives title from first line automatically.

### Custom Title
```bash
lmem remember "PostgreSQL 16 runs on port 5432 with WAL enabled" --title "Postgres Port Config"
```
* Sets custom title instead of first line.

---

## 3. Categories (`-t, --type`)

### Explicit Category
```bash
lmem remember "Use AES-GCM-256 for vault encryption" -t decision
```

### Supported Categories (14 Types)
| Category | Flag Value | Typical Use Case |
| :--- | :--- | :--- |
| `fact` | `-t fact` | Objective factual statements, environment parameters, constants. |
| `decision` | `-t decision` | Architecture choices, chosen libraries, agreed designs. |
| `instruction` | `-t instruction` | Workflow rules, formatting mandates, system prompts. |
| `preference` | `-t preference` | User habits, preferred themes, editor configurations. |
| `learning` | `-t learning` | Takeaways from debugging, error root causes, insights. |
| `goal` | `-t goal` | Milestone targets, sprint objectives, agent objectives. |
| `commitment` | `-t commitment` | Promises made to user, promised deadlines, deliverables. |
| `artifact` | `-t artifact` | File attachments, code snippets, diagrams, schemas. |
| `event` | `-t event` | Incidents, deployments, meetings, point-in-time milestones. |
| `relationship` | `-t relationship` | Inter-service dependencies, team hierarchies, mappings. |
| `observation` | `-t observation` | Unverified statements, conversational clues, chat facts. |
| `error` | `-t error` | Known bugs, failure modes, stack trace notes. |
| `context` | `-t context` | Background project context, domain terminology. |
| `password` | `-t password` | Sensitive tokens, passphrases, credentials. |

### Auto-Inference (When `-t` is omitted)
LightMem inspects the text and auto-assigns the category:
```bash
# Auto-infers [decision]
lmem remember "We decided to migrate from REST to gRPC"

# Auto-infers [instruction]
lmem remember "Always run cargo fmt before git commit"

# Auto-infers [preference]
lmem remember "I prefer dark mode in all IDEs"

# Auto-infers [artifact]
lmem remember -f ./schema.sql
```

---

## 4. Tags & Confidence

### Add Tags
```bash
lmem remember "Auth service runs on port 8180" --tags "auth,docker,backend"
```
* Comma-separated tags.
* Normalized to lowercase trimmed strings.

### Set Confidence Score
```bash
lmem remember "Server has 64GB ECC RAM" -c 0.98
```
* Flag: `-c, --confidence <FLOAT>` (range: `0.0` to `1.0`).
* Default: `0.9`.

---

## 5. Lifespan & Auto-Expiration (`--ttl`)

### Temporary Memories
```bash
# Expire in 30 seconds
lmem remember "Active debug session token: xyz" --ttl 30s

# Expire in 15 minutes
lmem remember "Deployment lock in progress" --ttl 15m

# Expire in 24 hours
lmem remember "Temporary staging host: 10.0.0.12" --ttl 24h

# Expire in 7 days
lmem remember "Feature branch testing active" --ttl 7d

# Expire in 2 weeks
lmem remember "Trial license key active" --ttl 2w
```
* Sets `expired_at` timestamp in UTC.
* Filtered out of active recall automatically once expired.

---

## 6. Conflict Superseding (`-s, --supersede`)

### Replace and Update Outdated Memories
```bash
lmem remember "PostgreSQL now runs on port 5433 with SSL" --title "Postgres Port" -s
```
* Scans active memories with cosine similarity $\ge 0.72$.
* Marks older conflicting memory as `expired`.
* Copies non-conflicting clauses into new record.
* Preserves and merges tags.

---

## 7. Knowledge Graph Links (`[[wikilinks]]`)

### Inline Graph Linking
```bash
lmem remember "API Gateway routes requests to [[Postgres Port Config]]"
```
* Detects Obsidian-style `[[Target Title]]` or `[[Target ID]]`.
* Automatically creates directed edge: `source ──references──▶ target` (weight: 1.0).
* Navigable via `lmem related` and `lmem recall --multi-hop`.

---

## 8. Entity Extraction Control

### Default: Automatic LLM Extraction (ON)
```bash
lmem remember "Alice lives at Flat 402, Hyderabad with pincode 500081"
```
* Invokes LLM in background.
* Extracts structured key-values into SQLite `entities` table:
  * `full_name`: Alice
  * `city`: Hyderabad
  * `pincode`: 500081
* Enables sub-millisecond exact answers via `lmem answer "what is alice pincode"`.

### Opt-Out of Entity Extraction (Fast Ingest)
```bash
lmem remember "Temporary scratch note" --no-entities
```
* Skips LLM entity extraction.
* Saves in sub-10ms without network or LLM calls.

---

## 9. Multimodal File Attachments (`-f, --file`)

### Attach Files as SQLite BLOBs
```bash
# Attach image
lmem remember "System architecture diagram" -f ./architecture.png

# Attach document
lmem remember "Q3 financial report" -f ./report.pdf --tags "finance,q3"

# Attach video
lmem remember "UI demo recording" -f ./demo.mp4

# Attach multiple files
lmem remember "Release assets" -f ./app.tar.gz -f ./checksums.txt
```
* Stored inside isolated SQLite `attachments` table (`0600` permissions).
* Computes SHA-256 digest for deduplication and tamper detection.
* Extracts metadata: MIME type, file size, image/video dimensions, media kind.
* Inferred category: `artifact`.
* Automatically appends media tag (e.g. `#image`, `#video`, `#code`).

### LLM Visual / Document Extraction (`-e, --extract`)
```bash
# Extract visual summary via configured LLM
lmem remember "Ingress diagram" -f ./ingress.png -e

# Extract with specific Ollama model
lmem remember "Whiteboard notes" -f ./notes.jpg -e -r ollama:qwen2.5:3b
```
* Invokes vision/multimodal model on file.
* Embeds extracted summary text directly into memory content for semantic search.

---

## 10. Provenance Tracking (`-p, --provenance`)

### Stamp Source Authenticity
```bash
lmem remember "CPU governor set to performance" -p "benchmarked"
```

Common Provenance Labels:
* `explicit_statement` (default for user CLI input)
* `validated` (verified by tests or benchmarks)
* `observed` (extracted from chat transcript)
* `corrected` (updated after user correction)
* `inferred` (deduced by agent)
* `imported` (brought from external dump)
* `attachment_metadata` (generated from file headers)
* `attachment_llm_extracted` (generated from visual/audio LLM pass)

---

## 11. Multi-Tenant Scopes & Connections

### Scoped Identity Storage
```bash
# Attribute memory to a named connection
lmem --connection agy remember "Use SQLite WAL mode"

# Override scope to agent level (isolated from user scope)
lmem --connection agy --scope agent remember "Local scratchpad note"

# Attribute memory to a specific session/chat ID
lmem --connection agy --session chat-982 remember "User prefers dark mode"
```
* Routes memory to scoped vault: `~/.lightmem/vaults/<hash>/memories.db`.
* Does not leak across unshared project/agent boundaries.

---

## 12. Vault Overrides

### Global Vault
```bash
lmem -g remember "Global developer preference: zsh"
```
* Forces storage in `~/.lightmem/memories.db` ignoring local git directory vaults.

### Custom Database Path
```bash
lmem --db /path/to/custom.db remember "Custom database entry"
```
* Targets specific SQLite database file directly.

---

## 13. Machine-Readable JSON Output (`--json`)

```bash
lmem remember "Production DB port is 5432" --tags "db,prod" --json
```

Output:
```json
{
  "id": "1ba8baf1-d080-4e1c-9d6a-9cc615747a5e",
  "title": "Production DB port is 5432",
  "content": "Production DB port is 5432",
  "category": "fact",
  "tags": ["db", "prod"],
  "confidence": 0.9,
  "status": "active",
  "provenance": "explicit_statement",
  "created_at": "2026-10-30T14:35:34.214550Z",
  "updated_at": "2026-10-30T14:35:34.214550Z",
  "expired_at": null,
  "entities": [
    { "name": "port", "val": "5432" }
  ],
  "attachments": []
}
```

---

## 14. Python SDK Equivalents

```python
from lmem import LightMem

# Initialize client
lm = LightMem()

# 1. Basic memory
mem = lm.remember(
    "PostgreSQL 16 runs on port 5432",
    title="Postgres Port",
    category="fact",
    tags=["db", "postgres"],
    confidence=0.95,
)

# 2. Memory with file attachment
diagram = lm.remember(
    "API routes to [[Postgres Port]]",
    files=["./architecture.png"],
    extract=False,
    tags=["architecture"],
)

# 3. Memory with TTL
temp = lm.remember(
    "Temporary test token",
    ttl="1h",
)

# 4. Scoped connection
scoped = LightMem(connection="agy", session="chat-101")
scoped.remember("Use WAL mode", category="decision")
```
