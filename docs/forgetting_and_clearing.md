# ❖ Forgetting & Clearing Guide (`lmem forget` & `lmem clear`)

Command-driven reference for expiring, permanently deleting, and resetting memory records in LightMem.

---

## 1. Syntax

```bash
# Expire or delete specific memory records (or all records with --all)
lmem forget [OPTIONS] [ID]

# Reset or wipe the entire memory database (aliases: purge, reset)
lmem clear [OPTIONS]
```

---

## 2. Soft-Retiring (Default Behavior)

### Expire Single Memory by ID or Prefix
```bash
# Expire by 8-character prefix
lmem forget 591b9c0f

# Expire by full UUID
lmem forget 591b9c0f-67f9-450a-b4f9-07daf1c20dae
```
* **Soft-Retirement**: Sets `status = 'expired'` and stamps `expired_at` timestamp.
* Excluded from active `lmem recall` and `lmem answer` queries immediately.
* Preserved in SQLite for historical point-in-time queries (`lmem recall --as-of <date>`).
* Visible when inspecting directly (`lmem inspect <ID>`) or listing expired records (`lmem list --status expired`).

---

## 3. Hard Deletion (`--hard`)

### Permanently Delete Single Memory
```bash
lmem forget 591b9c0f --hard
```
* Permanently deletes the row from the SQLite `memories` table.
* Purges associated vector embeddings from the vector index.
* Unlinks and removes all associated knowledge graph edges (`memory_links`).
* Deletes all isolated binary attachments from the SQLite BLOB vault.
* Irreversible: Cannot be recovered via `--as-of` time-travel queries.

---

## 4. Batch Operations (`--all`)

### Soft-Retire All Active Memories
```bash
lmem forget --all
```
* Marks all active memories as expired.
* Leaves SQLite schema, tables, and historical audit trail intact.

### Permanently Delete All Memories
```bash
lmem forget --all --hard
```
* Permanently wipes all memories, vectors, attachments, and links.
* Retains pristine database schema and table structure.

---

## 5. Database Resetting (`lmem clear`)

### Complete Vault Wipe (Aliases: `purge`, `reset`)
```bash
# Wipe database and re-initialize pristine empty WAL tables
lmem clear

# Alternative aliases
lmem purge
lmem reset
```
* Deletes database and WAL journal files on disk.
* Recreates fresh SQLite schema with empty indexes.

### Soft Clear (`--soft`)
```bash
lmem clear --soft
```
* Soft-retires all memories across the database without dropping or recreating database files.

---

## 6. Multi-Tenant Scopes & Connections

```bash
# Forget memory inside a named connection vault
lmem --connection agy forget 4156db6c

# Hard-delete memory inside an agent-isolated scope
lmem --connection agy --scope agent forget 4156db6c --hard

# Wipe an entire scoped connection database
lmem --connection agy clear
```

---

## 7. Vault Overrides

```bash
# Forget memory from global vault (~/.lightmem/memories.db)
lmem -g forget 4156db6c

# Target custom database file directly
lmem --db /path/to/vault.db forget 4156db6c --hard
lmem --db /path/to/vault.db clear
```

---

## 8. Machine-Readable JSON Output (`--json`)

### Forget JSON Output (`lmem forget --json`)
```bash
lmem forget --all --json
```

Output:
```json
{
  "database": "/Users/user/.lightmem/memories.db",
  "removed": 8,
  "hard_deleted": false,
  "attachments_purged": false
}
```

### Clear JSON Output (`lmem clear --json`)
```bash
lmem clear --json
```

Output:
```json
{
  "database": "/Users/user/.lightmem/memories.db",
  "removed": 8,
  "soft_retired": false
}
```

---

## 9. Python SDK Equivalents

```python
from lmem import LightMem

lm = LightMem()

# 1. Soft-retire single memory (preserves historical queries)
lm.forget("591b9c0f")

# 2. Hard permanent delete of single memory
lm.forget("591b9c0f", hard=True)

# 3. Soft-retire all active memories
lm.forget_all()

# 4. Hard delete all memories
lm.forget_all(hard=True)

# 5. Full vault wipe and re-initialization
lm.clear()

# 6. Soft clear vault
lm.clear(soft=True)
```
