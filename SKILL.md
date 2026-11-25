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
# 1. Store a memory (or pass `-` to read content from stdin)
lmem remember "PostgreSQL primary runs on port 5432 with WAL archiving enabled" \
  -t fact \
  --title "PostgreSQL Primary Config" \
  --tags "postgres,database,port,production" \
  --confidence 0.95 \
  --provenance validated \
  --supersede
echo "piped note" | lmem remember -

# 2. Store Files, Images, Audio & Documents (0600 BLOB vault)
lmem remember "Kubernetes ingress diagram" -f ./ingress.png
lmem remember -f ./standup_note.wav -f ./spec.pdf
lmem inspect <MEMORY_ID> --path                                 # Print absolute path of attachment
lmem inspect <ID1> <ID2> --paths                                # Print absolute paths across memories
lmem inspect <MEMORY_ID> --open                                 # Open attachment in default OS app

# 3. Hybrid Search & Similarity Filtering
lmem recall "postgres port" -l 5
lmem recall "postgres" --tag database --tag production          # Filter search by repeatable tags
lmem recall "service" --entity port=5432 --session chat-123     # Filter by entity and session ID
lmem recall "ingress.png" --media image                         # Filter by media kind (image|audio|video|document|code|archive|binary|any)
lmem recall "diagram" -m image -l 1 --path                      # Print file path of top match
lmem recall "service gateway" --multi-hop                       # Expand recall hits with 1-hop graph neighbors

# 4. List, Sort & Paginate Memories
lmem list                                                       # List active memories chronologically
lmem list --limit 10 --page 1 --sort confidence --json          # Sort (newest|oldest|updated|confidence) + JSON
lmem list --tag postgres --entity port --session chat-123       # Filter memories by tag, entity, and session
lmem list -t decision                                           # Filter memories by category
lmem list --status all                                          # View active, expired, or all memories

# 5. In-Place Update
lmem update <MEMORY_ID> --content "New text"                    # Update content and auto-re-embed
lmem update <MEMORY_ID> --title "Updated Title" --add-tag db --remove-tag tmp --clear-ttl
lmem update <MEMORY_ID> --ttl 7d --provenance validated -f ./notes.txt --refresh-entities

# 6. Knowledge Graph & Multi-Hop Traversal
lmem remember "API Gateway routes to [[PostgreSQL DB]]"         # Auto-links via [[wikilinks]]
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0              # Explicit directional relation
lmem links <MEMORY_ID>                                          # View direct incoming/outgoing edges
lmem unlink <SRC_ID> <DST_ID>                                   # Remove link
lmem related <MEMORY_ID> --hops 2                               # Multi-hop graph traversal
lmem graph --focus <MEMORY_ID> --hops 2                         # Visual subgraph view in terminal

# 7. Tags & Entities Management
lmem tags                                                       # List all distinct tags and memory counts
lmem tags "post"                                                # Search tags matching substring
lmem entities                                                   # List extracted entity names, values & counts
lmem entities port --memories                                   # Show memories referencing an entity

# 8. Neural Extractive QA & Grounded Answers
lmem answer "what port does postgresql use" --tag database      # Filtered pinpoint extraction
lmem answer "what is my pincode" --entity pincode --json

# 9. Conflict & Duplicate Resolution
lmem deduplicate                                                # Exact hash-based deterministic deduplication
lmem conflicts                                                  # Interactive semantic conflict review
lmem conflicts --yes                                            # Auto-merge detected conflicts via model
lmem conflicts --min-similarity 0.80 --json

# 10. Lifecycle, Archival & Restore
lmem inspect <MEMORY_ID> --json                                 # Deep inspection (aliases: get, show)
lmem forget <ID1> <ID2>                                         # Soft-retire one or more memories by ID/prefix/unique title
lmem forget <MEMORY_ID> --hard                                  # Permanently purge row + vector + FTS
lmem forget --all --yes                                         # Expire all memories (requires --yes, even with --json)
lmem restore <MEMORY_ID_OR_EXACT_TITLE>                         # Restore archived memory and clear TTL
lmem clear --yes                                                # Wipe entire database (requires --yes, even with --json)
lmem clear --soft --yes                                         # Soft-retire entire database

# 11. Conversational Observation & Ingestion
lmem observe "User prefers dark mode and JetBrains Mono" --yes  # Ingest conversation and extract facts
lmem observe -f ./transcript.jsonl --dry-run                    # Preview extracted candidates

# 12. Import / Export & DB Transfer Bundles
lmem export --json --tag postgres --status all -o ./subset.json # Filtered JSON or OKF export
lmem bundle export -o ./vault_backup.zip                        # Full vault backup (.zip with DB & all media)
lmem bundle info ./vault_backup.zip                             # Inspect bundle manifest and media counts
lmem bundle import ./vault_backup.zip                           # Restore entire vault
lmem bundle import ./vault_backup.zip --merge                   # Merge into existing vault

# 13. Context, Identity & Multi-Vault Management
lmem context                                                    # View active connection, scope, and DB path
lmem status                                                     # View all managed vaults and connections
lmem identity list connections                                  # List registered connections
lmem identity connection-update <ID> --memory-scope project     # Update connection project or scope
lmem identity connection-remove <ID>                            # Remove registered connection (also user-remove/agent-remove/harness-remove)

# 14. Command History & Audit Trail
lmem history                                                    # View recent command execution history
lmem history "answer"                                           # Search history for commands matching query
lmem history --status error -n 10                               # Filter by status (success|error) with limit
lmem history --clear                                            # Clear command execution history

# 15. Engine Configuration
lmem setup                                                      # Interactive setup wizard
lmem config                                                     # View active settings
lmem config path                                                # Print active config file path
lmem config get reranker                                        # Read a single config field (validates key)
lmem config set entity_extraction on                            # Set a single validated config field
lmem config --backend onnx|ollama|hash                          # Set embedding backend
lmem config --embedding-model <model>                           # Set vector embedding model (do NOT use --model)
lmem config --answer-model <model>                              # Set chat/QA model (e.g. gpt-4o-mini)
lmem config --entity-extraction on|off                          # Toggle entity extraction
lmem config --endpoint <url>                                    # Set OpenAI/LLM endpoint URL
lmem config --model-api-key <key>                               # Set LLM/model API key (--api-key sets remote server auth)
lmem config reload                                              # Reload settings from disk
lmem config reset --yes                                         # Reset config.json to defaults
lmem reindex --yes                                              # Rebuild embeddings after model change
lmem doctor                                                     # Run system diagnostics

# 16. Remote Server & Client Routing
lmem serve --host 127.0.0.1 --port 8080 --api-key "secret"      # Run server locally (pass --host 0.0.0.0 for LAN access)
lmem serve -d --host 0.0.0.0 --port 8080 --api-key "secret"     # Run as background daemon
lmem serve --status --json                                      # Check background daemon status (human or JSON)
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
| `observation` | Patterns, behavioral signals, recurring observed context | `0.85 - 0.95` | `observed` |
| `error` | Crash signatures, stack traces, regression postmortems | `0.90 - 1.00` | `validated` / `corrected` |
| `context` | Current workspace state, active branch, session notes | `0.80 - 0.95` | `validated` |
| `password` | Credentials, DSNs with embedded auth, secret references | `0.95 - 1.00` | `explicit_statement` |

---

## 3. Best Practices & Flag Disambiguation for AI Agents

1. **Write Atomic, Declarative Facts**: Store self-contained, specific statements rather than vague notes.
2. **Use `lmem update` for Modifying Existing Facts**: Retains original UUID, creation timestamps, and graph relations while re-indexing vectors automatically; supports `--add-tag`, `--remove-tag`, `--ttl`, `--clear-ttl`, `--provenance`, and `-f/--file`.
3. **Use `--supersede` When Overwriting Conflicting Facts**: Retires outdated conflicting records automatically during `remember`.
4. **Disambiguate `--model` and API Keys in `lmem config`**:
   - Vector search model: `--embedding-model` (or `--onnx-model`).
   - Generative/QA model: `--answer-model` (`--openai-model`, `--chat-model`).
   - Remote LightMem server key: `--api-key` (`--remote-api-key`). LLM/provider key: `--model-api-key` (`--openai-api-key`, `--ollama-api-key`).
   - In `lmem remember` / `answer` / `conflicts` / `observe`: `-m` / `--model` refers to the answer/extraction model.
5. **Use `--session` Standalone or with `--connection`**: `--session <ID>` tags new memories on `remember` and filters reads on `recall`, `list`, `answer`, and `export` even without `--connection`.
6. **Always Pass `--yes` with `lmem observe`**: Non-interactive `observe` with `--json` defaults to a preview/dry-run unless `--yes` is explicitly combined (`--yes --json`).
7. **Destructive Operations Mandate `--yes` (Even with `--json`)**: `lmem clear`, `lmem forget --all`, and `lmem config --reset-db` require `--yes` to proceed without interactive confirmation.
8. **Scope Project vs Global Memories**: Local `./.lightmem.db` takes precedence. Use `-g` (`--global`) for user-level global memories.
