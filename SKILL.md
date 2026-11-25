---
name: lightmem
description: Ultra-fast local AI agent persistent memory engine (`lmem`). Use this skill to store, search, answer questions from, attach/retrieve multimodal files (images, audio, video, documents) with `--path`/`--paths`, update in-place, manage tags and links, deduplicate/merge conflicts, and export/import persistent memories across sessions using SQLite WAL + FTS5 BM25 + ONNX/Ollama vector embeddings and Extractive QA reranking.
---

# LightMem (`lmem`) Skill

Operational reference for using the `lmem` CLI to persist, retrieve, query, update, attach media/files, and maintain agent memories across sessions.

---

## Quick Installation

```bash
# macOS & Linux
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

# Python pip
pip install -U lmem
```

---

## 1. Core CLI Commands

Execute `lmem` via shell commands. Never simulate memory storage.

```bash
# 1. Store a memory
lmem remember "PostgreSQL primary runs on port 5432 with WAL archiving enabled" \
  -t fact \
  --title "PostgreSQL Primary Config" \
  --tags "postgres,database,port,production" \
  --confidence 0.95 \
  --provenance validated \
  --supersede

# 2. Store Files, Images, Audio & Documents (0600 BLOB vault)
lmem remember "Kubernetes ingress diagram" -f ./ingress.png
lmem remember -f ./standup_note.wav -f ./spec.pdf
lmem inspect <MEMORY_ID> --path                                 # Print absolute path of attachment
lmem inspect <ID1> <ID2> --paths                                # Print absolute paths across memories
lmem inspect <MEMORY_ID> --open                                 # Open attachment in default OS app

# 3. Hybrid Search & Similarity Filtering
lmem recall "postgres port" -l 5
lmem recall "postgres" --tag database                           # Filter search by tag
lmem recall "ingress.png" --media image                         # Filter by media kind (image|audio|video|document|code|any)
lmem recall "diagram" -m image -l 1 --path                      # Print file path of top match
lmem recall "service gateway" --multi-hop                       # Expand recall hits with 1-hop graph neighbors

# 4. List & Paginate Memories
lmem list                                                       # List active memories chronologically
lmem list --limit 10 --page 1 --json                            # Paginated JSON output
lmem list --tag postgres                                        # Filter memories by tag
lmem list -t decision                                           # Filter memories by category
lmem list --status expired                                      # View archived / soft-deleted memories

# 5. In-Place Update
lmem update <MEMORY_ID> --content "New text"                    # Update content and auto-re-embed
lmem update <MEMORY_ID> --title "Updated Title" --tags "db,sql" # Update metadata in-place

# 6. Knowledge Graph & Multi-Hop Traversal
lmem remember "API Gateway routes to [[PostgreSQL DB]]"         # Auto-links via [[wikilinks]]
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0              # Explicit directional relation
lmem links <MEMORY_ID>                                          # View direct incoming/outgoing edges
lmem unlink <SRC_ID> <DST_ID>                                   # Remove link
lmem related <MEMORY_ID> --hops 2                               # Multi-hop graph traversal
lmem graph --focus <MEMORY_ID> --hops 2                         # Visual subgraph view in terminal

# 7. Tags Management
lmem tags                                                       # List all distinct tags and memory counts
lmem tags "post"                                                # Search tags matching substring

# 8. Neural Extractive QA & Grounded Answers
lmem answer "what port does postgresql use"                     # Pinpoint fact extraction from memory index
lmem answer "what is my pincode" --json

# 9. Conflict & Duplicate Resolution
lmem deduplicate                                                # Exact hash-based deterministic deduplication
lmem conflicts                                                  # Interactive semantic conflict review
lmem conflicts --yes                                            # Auto-merge detected conflicts via model
lmem conflicts --min-similarity 0.80 --json

# 10. Lifecycle, Archival & Restore
lmem inspect <MEMORY_ID> --json                                 # Deep inspection (aliases: get, show)
lmem forget <MEMORY_ID>                                         # Soft-retire (status -> expired)
lmem forget <MEMORY_ID> --hard                                  # Permanently purge row + vector + FTS
lmem forget --all --yes                                         # Expire all memories (requires --yes)
lmem restore <MEMORY_ID_OR_EXACT_TITLE>                         # Restore archived memory and clear TTL
lmem clear --yes                                                # Wipe entire database (requires --yes)
lmem clear --soft --yes                                         # Soft-retire entire database

# 11. Conversational Observation & Ingestion
lmem observe "User prefers dark mode and JetBrains Mono" --yes  # Ingest conversation and extract facts
lmem observe -f ./transcript.jsonl --dry-run                    # Preview extracted candidates

# 12. Import / Export & DB Transfer Bundles
lmem bundle export -o ./vault_backup.zip                        # Full vault backup (.zip with DB & all media)
lmem bundle info ./vault_backup.zip                             # Inspect bundle manifest and media counts
lmem bundle import ./vault_backup.zip                           # Restore entire vault
lmem bundle import ./vault_backup.zip --merge                   # Merge into existing vault

# 13. Context, Identity & Multi-Vault Management
lmem context                                                    # View active connection, scope, and DB path
lmem status                                                     # View all managed vaults and connections
lmem identity list connections                                  # List registered connections

# 14. Command History & Audit Trail
lmem history                                                    # View recent command execution history
lmem history "answer"                                           # Search history for commands matching query
lmem history --status error -n 10                               # Filter by status (success|error) with limit
lmem history --clear                                            # Clear command execution history

# 15. Engine Configuration
lmem setup                                                      # Interactive setup wizard
lmem config                                                     # View active settings
lmem config --backend onnx|ollama|hash                          # Set embedding backend
lmem config --embedding-model <model>                           # Set vector embedding model (do NOT use --model)
lmem config --answer-model <model>                              # Set chat/QA model (e.g. gpt-4o-mini)
lmem config --entity-extraction on|off                          # Toggle entity extraction
lmem config --endpoint <url>                                    # Set OpenAI/LLM endpoint URL
lmem config --api-key <key>                                     # Set API key for model service
lmem config reload                                              # Reload settings from disk
lmem reindex --yes                                              # Rebuild embeddings after model change
lmem doctor                                                     # Run system diagnostics

# 16. Remote Server & Client Routing
lmem serve --host 0.0.0.0 --port 8080 --api-key "secret"        # Run server in foreground
lmem serve -d --host 0.0.0.0 --port 8080 --api-key "secret"     # Run as background daemon
lmem serve --status                                             # Check background daemon status
lmem serve --stop                                               # Stop background daemon
lmem remote http://server:8080 --api-key "secret"               # Connect to remote LightMem
lmem local                                                      # Disconnect and return to local mode
```

---

## 2. Memory Categories (`-t` / `--type`)

| Category | When to Use | Recommended Confidence | Typical Provenance |
| :--- | :--- | :--- | :--- |
| `fact` | Verified system facts, ports, URLs, personal/identity details | `0.90 - 1.00` | `validated` / `explicit_statement` |
| `decision` | Architectural choices, stack selections, design trade-offs | `0.90 - 1.00` | `explicit_statement` |
| `instruction` | Standing user rules, coding conventions, mandatory constraints | `0.95 - 1.00` | `explicit_statement` |
| `preference` | User workflow, UI/UX, editor, or communication preferences | `0.85 - 1.00` | `explicit_statement` |
| `learning` | Post-debugging insights, workarounds, root causes | `0.85 - 0.95` | `validated` / `corrected` |
| `goal` | Project milestones, OKRs, long-term targets | `0.85 - 1.00` | `explicit_statement` |
| `commitment` | Action items, TODOs, deliverables | `0.90 - 1.00` | `explicit_statement` |
| `artifact` | File paths, schemas, build outputs | `0.90 - 1.00` | `validated` |
| `event` | Incidents, deployments, migrations | `0.85 - 0.95` | `validated` |
| `relationship` | Ownership, team mapping, service dependencies | `0.85 - 0.95` | `explicit_statement` |
| `error` | Crash signatures, stack traces, regression postmortems | `0.90 - 1.00` | `validated` / `corrected` |
| `context` | Current workspace state, active branch, session notes | `0.80 - 0.95` | `validated` |
| `password` | Credentials, DSNs with embedded auth, secret references | `0.95 - 1.00` | `explicit_statement` |

---

## 3. Best Practices & Flag Disambiguation for AI Agents

1. **Write Atomic, Declarative Facts**: Store self-contained, specific statements rather than vague notes.
2. **Use `lmem update` for Modifying Existing Facts**: Retains original UUID, creation timestamps, and graph relations while re-indexing vectors automatically.
3. **Use `--supersede` When Overwriting Conflicting Facts**: Retires outdated conflicting records automatically during `remember`.
4. **Disambiguate `--model`**:
   - In `lmem config`: Always use `--embedding-model` for vector search models. Use `--answer-model` for generative LLM/reranker models.
   - In `lmem remember` / `answer` / `conflicts` / `observe`: `-m` / `--model` refers to the answer/extraction model.
5. **Always Pass `--yes` with `lmem observe`**: Non-interactive `observe` with `--json` defaults to a preview/dry-run unless `--yes` is explicitly combined (`--yes --json`).
6. **Destructive Operations Mandate `--yes`**: `lmem clear` and `lmem forget --all` require `--yes` to proceed without interactive confirmation.
7. **Scope Project vs Global Memories**: Local `./.lightmem.db` takes precedence. Use `-g` (`--global`) for user-level global memories.
