---
name: lightmem
description: Ultra-fast local AI agent persistent memory engine (`lmem`). Use this skill to store, search, answer questions from, deduplicate/merge conflicts in, and export/import persistent memories across sessions using SQLite WAL + FTS5 BM25 + ONNX/Ollama vector embeddings and Extractive QA reranking.
---

# LightMem (`lmem`) Skill

Complete operational reference for using the `lmem` CLI to persist, retrieve, query, and maintain long-term agent memories across sessions.

---

## 1. Quick Command Reference

Always execute `lmem` via shell commands. Never simulate memory storage or rely solely on ephemeral conversation history for long-term facts, preferences, or architectural decisions.

```bash
# 1. Store a memory (auto-infers category if -t is omitted; auto-deduplicates exact matches)
lmem remember "PostgreSQL primary runs on port 5432 with WAL archiving enabled" \
  -t fact \
  --title "PostgreSQL Primary Config" \
  --tags "postgres,database,port,production" \
  --confidence 0.95 \
  --provenance validated \
  --supersede

# 2. Hybrid Search (SQLite FTS5 BM25 + Vector Cosine RRF + Acronym Boost)
lmem recall "postgres port" -l 5
lmem recall "postgres port" -t fact --min-similarity 0.5 --json

# 3. Extractive QA / LLM Grounded Answer (extracts the exact span from top candidates)
lmem answer "what port does postgresql use"
lmem answer "what is my pan card no" --json
lmem answer "who is kk" -r ollama:qwen2.5:3b

# 4. Temporal & Single-Day Filtering (--as-of vs --date)
lmem recall "deployment" --as-of 2026-10-01        # Cumulative state at that point in time
lmem recall "deployment" --date 2026-10-05         # Memories created/updated on that exact UTC day
lmem list --date 2026-10-05 -l 20                  # Chronological list for a single day
lmem answer "what changed today" --date 2026-10-05

# 5. Paginated Chronological Listing
lmem list --page 1 -l 20 -t decision --status active --json

# 6. Nearest-Neighbor Conflict & Duplicate Resolution
lmem conflicts                                     # Interactive [y]es / [n]o / [a]ll / [q]uit review
lmem conflicts --yes                               # Auto-merge all detected conflicts via active reranker
lmem conflicts --min-similarity 0.80 --json        # Inspect conflict pairs as JSON without merging

# 7. Lifecycle & Expiration
lmem remember "Staging deploy freeze until Friday" -t commitment --ttl 48h
lmem get <MEMORY_ID> --json
lmem forget <MEMORY_ID>                            # Soft-retire (status -> expired, keeps audit trail)
lmem forget <MEMORY_ID> --hard                     # Permanently purge row + vector + FTS index

# 8. Import / Export (Open Knowledge Format v2, JSONL, Mem0, LangChain, Letta, MCP Graph)
lmem export ./memory_bundle                        # Export human-readable OKF v2 Markdown tree
lmem export ./backup.json --json                   # Full lossless JSON backup envelope
lmem import ./backup.json --enrich                 # Transactional import with heuristic enrichment

# 9. Engine & Reranker Configuration
lmem config                                        # View active Embedder, Reranker, Ollama URL, Config path
lmem config --reranker minilm-squad2               # Default local ONNX Extractive QA (~6ms, 127MB)
lmem config --reranker tinyroberta-squad2          # High-accuracy RoBERTa ONNX Extractive QA (~11ms)
lmem config --reranker ollama:qwen2.5:3b           # Local Ollama generative QA & conflict merger
lmem config --reranker top1                        # 0ms instant vector rank-1 return
lmem config --backend onnx --onnx-model bge-small  # Switch embedding model (prompts for atomic reindex)
lmem config --download all                         # Pre-cache ONNX embedding + QA models for offline use
```

---

## 2. The 14 Memory Categories (`-t` / `--type`)

If `--type` (`-t`) is omitted on `lmem remember`, LightMem automatically infers the category from explicit prefixes (`Decision:`, `Error:`, `TODO:`) or content heuristics. Passing `-t` explicitly is recommended for agent precision:

| Category | When to Use | Recommended Confidence | Typical Provenance | Example |
| :--- | :--- | :--- | :--- | :--- |
| `fact` | Verified system facts, IDs, ports, URLs, personal/identity details | `0.90 - 1.00` | `validated` / `explicit_statement` | `"PAN ID is HAQPP8118D"` |
| `decision` | Architectural choices, stack selections, design trade-offs | `0.90 - 1.00` | `explicit_statement` / `inferred` | `"Use SQLite WAL mode with FTS5 for local storage"` |
| `instruction` | Standing user rules, coding conventions, mandatory constraints | `0.95 - 1.00` | `explicit_statement` | `"Never use hardcoded vendor token regexes in inference"` |
| `preference` | User workflow, UI/UX, editor, or communication preferences | `0.85 - 1.00` | `explicit_statement` / `observed` | `"Deliver direct punchy answers with zero fluff"` |
| `learning` | Post-debugging insights, non-obvious workarounds, root causes | `0.85 - 0.95` | `observed` / `corrected` | `"PowerShell ErrorActionPreference=Stop aborts on stderr progress bars"` |
| `goal` | Project milestones, OKRs, long-term targets | `0.85 - 1.00` | `explicit_statement` | `"Achieve sub-10ms local extractive QA on CPU"` |
| `commitment` | Action items, TODOs, promised deliverables | `0.90 - 1.00` | `explicit_statement` | `"Add Windows MSVC release artifact to v0.2.0"` |
| `artifact` | File paths, generated reports, schemas, build outputs | `0.90 - 1.00` | `observed` | `"Release binaries are published via .github/workflows/release.yml"` |
| `event` | Incidents, deployments, migrations, dated occurrences | `0.85 - 0.95` | `observed` | `"Migrated kk-Linux vault to v0.2.0 on 2026-10-05"` |
| `relationship` | Ownership, team mapping, service-to-service dependencies | `0.85 - 0.95` | `explicit_statement` | `"API Gateway routes /v1/auth to Keycloak cluster"` |
| `observation` | Empirical runtime measurements, benchmark findings | `0.75 - 0.90` | `observed` | `"minilm-squad2 ONNX latency averages 6.1ms per query on CPU"` |
| `error` | Crash signatures, stack traces, regression postmortems | `0.90 - 1.00` | `observed` / `corrected` | `"Fastly IPv6 node 2a04:4e42:5a::649 returns HTTP 503 on kk-Linux"` |
| `context` | Current workspace state, active branch, session handoff notes | `0.80 - 0.95` | `observed` | `"Active branch is feat/extractive-qa-ollama"` |
| `password` | Credentials, DSNs with embedded auth, secret references | `0.95 - 1.00` | `explicit_statement` | `"Internal staging DB URI uses basic auth on port 5432"` |

---

## 3. Provenance & Confidence Guidelines

### Provenance (`--provenance`)
- `explicit_statement` *(default)*: Directly stated by the user.
- `validated`: Verified against live code, tests, or command output.
- `observed`: Witnessed during command execution, benchmarks, or logs.
- `corrected`: Recorded after fixing a bug or user correction.
- `inferred`: Derived logically from codebase patterns.
- `imported`: Ingested from external files (`lmem import`).

### Confidence (`--confidence <0.0-1.0>`)
- `1.0`: Explicit user rule, identity fact, or verified test assertion.
- `0.9 - 0.95`: Validated architecture decision or reproducible technical fact.
- `0.8 - 0.85`: Strong observation or preference.
- `< 0.6`: Do not store unverified guesses as permanent memories; use `--ttl` if storing temporary working state.

---

## 4. Choosing Between `recall`, `answer`, and `conflicts`

| Command | Output | When to Use |
| :--- | :--- | :--- |
| `lmem recall "<query>"` | Top-$K$ full memory cards with RRF score (`BM25 + Vector + Acronym Boost`), ID, category, and tags | When gathering broad context before planning, coding, or debugging |
| `lmem answer "<question>"` | Exact extracted span (via ONNX `minilm-squad2` / `tinyroberta-squad2` or `ollama:<model>`), confidence `%`, and source memory ID | When answering a specific factual question (`"what port..."`, `"what is my..."`, `"who is..."`) |
| `lmem conflicts` | Pairs of semantically similar + lexically overlapping memories with `[OLDER · UTC]` and `[NEWER · UTC]` timestamps | When cleaning up duplicate memories or merging updated facts into a single canonical record |

---

## 5. Best Practices for AI Agents

1. **Write Atomic, Declarative Facts**:
   - **Bad**: `"User asked me to fix the Windows installer and I found out stderr caused an error"`
   - **Good**: `"Windows PowerShell $ErrorActionPreference='Stop' treats native command stderr progress bars as terminating errors; wrap stderr-emitting CLI calls with 'Continue'."`
2. **Use `--supersede` When Updating a Fact**:
   - Passing `--supersede` on `lmem remember` automatically checks for conflicting active memories and retires the outdated record in the same transaction.
3. **Pass `--json` for Programmatic Parsing**:
   - `lmem recall`, `lmem answer`, `lmem list`, `lmem get`, `lmem stats`, and `lmem conflicts` all support `--json` with clean, deterministic schemas.
4. **Scope Project vs Global Memories**:
   - By default, `lmem` uses `./.lightmem.db` if present in the current directory or git root, falling back to `~/.lightmem/memories.db`. Pass `-g` (`--global`) to force the global vault or `--db <PATH>` for an isolated vault.
