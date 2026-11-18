---
name: lightmem
description: Ultra-fast local AI agent persistent memory engine (`lmem`). Use this skill to store, search, answer questions from, attach/retrieve multimodal files (images, audio, video, documents) with `--path`/`--paths`, deduplicate/merge conflicts in, and export/import persistent memories across sessions using SQLite WAL + FTS5 BM25 + ONNX/Ollama vector embeddings and Extractive QA reranking.
---

# LightMem (`lmem`) Skill

Operational reference for using the `lmem` CLI to persist, retrieve, query, attach media/files, and maintain agent memories across sessions.

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
lmem recall "ingress.png" --media image                         # Filter by media kind (image|audio|video|document|code|any)
lmem recall "diagram" -m image -l 1 --path                      # Print file path of top match
lmem recall "service gateway" --multi-hop                       # Expand recall hits with 1-hop graph neighbors

# 4. Knowledge Graph & Multi-Hop Traversal
lmem remember "API Gateway routes to [[PostgreSQL DB]]"         # Auto-links via [[wikilinks]]
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0              # Explicit directional relation
lmem unlink <SRC_ID> <DST_ID>                                   # Remove link
lmem related <MEMORY_ID> --hops 2                               # Multi-hop graph traversal
lmem graph --focus <MEMORY_ID> --hops 2                         # Visual subgraph view in terminal

# 5. Neural Extractive QA & Grounded Answers
lmem answer "what port does postgresql use"                     # Pinpoint fact extraction from memory index
lmem answer "what is my pincode" --json

# 6. Conflict & Duplicate Resolution
lmem conflicts                                                  # Interactive review
lmem conflicts --yes                                            # Auto-merge detected conflicts via model
lmem conflicts --min-similarity 0.80 --json

# 7. Lifecycle & Deletion
lmem get <MEMORY_ID> --json
lmem forget <MEMORY_ID>                                         # Soft-retire (status -> expired)
lmem forget <MEMORY_ID> --hard                                  # Permanently purge row + vector + FTS

# 8. Import / Export & DB Transfer Bundles
lmem bundle export -o ./vault_backup.zip                        # Full vault backup (.zip with DB & all media)
lmem bundle info ./vault_backup.zip                             # Inspect bundle manifest and media counts
lmem bundle import ./vault_backup.zip                           # Restore entire vault
lmem bundle import ./vault_backup.zip --merge                   # Merge into existing vault

# 9. Command History & Audit Trail
lmem history                                                    # View recent command execution history
lmem history "answer"                                           # Search history for commands matching query
lmem history --status error -n 10                               # Filter by status (success|error) with limit
lmem history --clear                                            # Clear command execution history

# 10. Engine Configuration
lmem setup                                                      # Interactive setup wizard
lmem config                                                     # View active settings
lmem config --backend onnx|ollama|hash                          # Set embedding backend
lmem config --embedding-model <model>                           # Set vector embedding model
lmem config --openai-model <model>                              # Set chat/QA model (e.g. gpt-4o-mini, ollama:qwen2.5:0.5b)
lmem config --entity-extraction on|off                          # Toggle entity extraction
lmem config --endpoint <url>                                    # Set OpenAI/LLM endpoint URL
lmem config --api-key <key>                                     # Set API key for model service
lmem config reload                                              # Reload settings from ~/.lightmem/config.json
lmem doctor                                                     # Run system diagnostics

# 11. Remote Server & Client Routing
lmem serve --host 0.0.0.0 --port 8080 --api-key "secret"        # Run server in foreground
lmem serve -d --host 0.0.0.0 --port 8080 --api-key "secret"     # Run as background daemon
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

## 3. Best Practices for AI Agents

1. **Write Atomic, Declarative Facts**: Store self-contained, specific statements rather than vague notes.
2. **Use `--supersede` When Updating Facts**: Retires outdated conflicting records automatically.
3. **Pass `--json` or `--path` for Scripts**: Clean, deterministic JSON output or raw file paths for tools.
4. **Scope Project vs Global Memories**: Local `./.lightmem.db` takes precedence. Use `-g` (`--global`) for global memories.
