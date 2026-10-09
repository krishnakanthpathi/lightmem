---
name: lightmem
description: Ultra-fast local AI agent persistent memory engine (`lmem`). Use this skill to store, search, answer questions from, deduplicate/merge conflicts in, and export/import persistent memories across sessions using SQLite WAL + FTS5 BM25 + ONNX/Ollama vector embeddings and Extractive QA reranking.
---

# LightMem (`lmem`) Skill

Complete operational reference for using the `lmem` CLI to persist, retrieve, query, and maintain long-term agent memories across sessions.

---

## Prerequisites & Installation

If `lmem` is not installed or not found on PATH (`which lmem` fails), install it before running memory commands:

```bash
# Option 1: macOS & Linux (curl shell installer)
curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.sh | sh

# Option 2: Windows PowerShell
irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex

# Option 3: Python pip (bundles or auto-downloads binary to ~/.lightmem/bin)
pip install -U lmem

# Option 4: Cargo (build from source)
cargo install --git https://github.com/krishnakanthpathi/lightmem.git
```

Verify binary availability:
```bash
lmem --version
```

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

# 1b. Conversational Ingestion & Observation (Extract candidate memories with confirmation)
lmem observe "User: I prefer dark mode and JetBrains Mono.\nAssistant: Noted!" # Interactive review [y/n/a/q]
lmem observe transcript.txt                            # Ingest transcript file with interactive review
cat dialogue.jsonl | lmem observe                      # Pipe conversation via stdin
lmem observe "User: PostgreSQL runs on port 5433" --yes # Confirm & persist all candidates immediately
lmem observe chat.txt --dry-run                        # Preview candidate memories without saving
lmem observe chat.txt --dry-run --json                 # Export candidates as JSON for agent inspection
lmem observe chat.txt -r ollama:qwen2.5:3b             # Use LLM-assisted semantic extraction (aliases: observed, extract)

# 2. Hybrid Search & Similarity Filtering (SQLite FTS5 BM25 + Vector Cosine RRF + Acronym Boost)

lmem recall "postgres port" -l 5
lmem recall "postgres port" -t fact --min-similarity 0.5 --json
lmem recall "new architectural idea" --min-similarity 0.70 -l 50 --json # Count & find semantically similar existing memories
lmem recall "service gateway" --multi-hop          # Expands recall hits with 1-hop graph neighbors

# 3. Knowledge Graph, Linking & Multi-Hop Traversal
lmem autolink                                      # Auto-link vault via title mentions, shared tags, and vector similarity
lmem autolink --min-similarity 0.75 --json         # Configure cosine similarity threshold (creates weighted relates_to edges)
lmem remember "API Gateway routes to [[PostgreSQL DB]]" # Auto-links via [[wikilinks]]
lmem link <SRC_ID> <DST_ID> -r "depends_on" -w 1.0  # Explicit directional relation
lmem unlink <SRC_ID> <DST_ID>                       # Remove link
lmem related <MEMORY_ID> -n 1 --json               # List & count direct 1-hop similar/connected memories
lmem related <MEMORY_ID> --hops 2                  # Multi-hop graph traversal with hop-decay scoring
lmem graph                                         # ASCII/Unicode network tree in terminal
lmem graph --focus <MEMORY_ID> --hops 2            # Neighborhood subgraph view
lmem graph --focus <MEMORY_ID> -n 1 --json         # Focused neighborhood node & edge counts as JSON
lmem graph --json                                  # Export graph nodes and edges as JSON

# 4. Extractive QA / LLM Grounded Answer (extracts the exact span from top candidates)
lmem answer "what port does postgresql use"
lmem answer "what is my pan card no" --json
lmem answer "who is kk" -r ollama:qwen2.5:3b

# 5. Temporal & Single-Day Filtering (--as-of vs --date)
lmem recall "deployment" --as-of 2026-10-01        # Cumulative state at that point in time
lmem recall "deployment" --date 2026-10-05         # Memories created/updated on that exact UTC day
lmem list --date 2026-10-05 -l 20                  # Chronological list for a single day
lmem answer "what changed today" --date 2026-10-05

# 6. Paginated Chronological Listing
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

# 9. Engine, Reranker & Setup Wizard
lmem setup                                         # Interactive CLI wizard: Local Ollama, Ollama Cloud, Custom, or Offline
lmem setup --preset ollama-local                   # Configure Local Ollama LLM backend
lmem setup --preset local                          # Configure 100% offline local ONNX mode
lmem config                                        # View active settings & available setup presets
lmem config --json                                 # Machine-readable JSON configuration
lmem config --preset local                         # Preset 1: 100% offline (bge-small ONNX + minilm-squad2 QA)
lmem config --preset ollama-cloud                  # Preset 2: Local ONNX embeddings + Ollama gemma4:31b-cloud reranker
lmem config --preset ollama-local                  # Preset 3: Local ONNX embeddings + Ollama localhost LLM reranker
lmem config --download all                         # Pre-cache ONNX embedding + QA models for offline use
lmem answer "test question" -r minilm-squad2       # Per-query local ONNX override
lmem answer "test question" -r ollama:gemma4:31b-cloud # Per-query Ollama override

# 10. AI Agent Platform Connection (dumps LightMem SKILL.md folder)
lmem connect                                       # Auto-detect all installed agent platforms and deploy skill
lmem connect antigravity                           # Deploy to Google Antigravity (~/.gemini/config/skills/lightmem/SKILL.md)
lmem connect codex                                 # Deploy to OpenAI Codex (~/.codex/skills/lightmem/SKILL.md)
lmem connect hermes                                # Deploy to Hermes Agent (~/.hermes/skills/productivity/lightmem/SKILL.md)
lmem connect cursor                                # Deploy to Cursor (.cursor/rules/lightmem.mdc)
lmem connect all                                   # Deploy to all supported agent platforms
lmem connect --list                                # Inspect detected agent platforms on the system
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
| `context` | Current workspace state, active branch, session handoff notes | `0.80 - 0.95` | `observed` | `"Active branch is main"` |
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

## 4. Knowledge Graph, Wikilinks & Interactive Graph View

LightMem features a native SQLite graph layer (`memory_links`) with $O(1)$ indexing, foreign key cascading, and zero external dependencies:

1. **Obsidian-Grade Wikilinks**:
   - Wrap any memory title or ID in `[[...]]` inside content (e.g. `[[PostgreSQL DB]]` or `[[db-primary|Main Database]]`).
   - LightMem automatically parses and binds bidirectional links on `lmem remember`.
2. **Explicit Directional Linking**:
   - `lmem link <src> <dst> -r <relation> [-w <weight>]`
   - Common relations: `relates_to`, `depends_on`, `references`, `implements`, `causes`, `bypasses`.
   - Weights scale hop-decay scores ($1.0 / \text{hop} \times \text{weight}$).
   - Remove connections with `lmem unlink <src> <dst> [-r <relation>]`.
3. **Multi-Hop Traversal (`lmem related`)**:
   - Breadth-first graph search up to $N$ hops (`lmem related <id> --hops 2`).
   - Cycle-safe with hop-distance attenuation scoring ($1.0 / \text{hop}$).
   - `lmem recall "<query>" --multi-hop` expands semantic recall hits with 1-hop connected neighbors.
4. **Interactive Force-Directed Visualizer & Topology (`lmem graph`)**:
   - Terminal network tree or JSON export.
   - CLI flags: `lmem graph` (terminal Unicode tree), `lmem graph --focus <id> --hops 2` (neighborhood subgraph), `lmem graph --json` (export nodes and edges array).
5. **Graph Similarity Search & Counting Similar Memories**:
   - **Pre-Insert / Query Similarity Count**:
     Use `lmem recall "<query_or_text>" --min-similarity <0.1-1.0> -l <N> --json` to inspect and count existing memories with high cosine similarity before deciding whether to store, link, or supersede.
   - **Vault-Wide Semantic Auto-Linking (`lmem autolink`)**:
     LightMem scans all active memories across three similarity dimensions:
     1. *Semantic Vector Cosine Similarity*: Memory pairs with $\text{sim} \ge \text{min\_similarity}$ (default `0.75`) automatically receive bidirectional `relates_to` edges with edge weight equal to their cosine similarity ($0.10 - 1.00$).
     2. *Title Mentions*: Content mentioning another active memory's exact non-generic title receives a `mentions` edge ($w = 1.0$).
     3. *Shared Topics*: Memories sharing $\ge 2$ non-generic tags receive bidirectional `shared_topic` edges ($w = 0.8$).
     Output reports: `New links created: X | Total nodes: Y | Total edges: Z`.
   - **Counting Connected Similar Neighbors for an Existing Node**:
     - `lmem related <ID> -n 1 --json` outputs all direct 1-hop connected memories, their relation types (`relates_to`, `mentions`, `shared_topic`, custom), and similarity weights.
     - `lmem graph --focus <ID> -n 1 --json` returns the exact subgraph `{ nodes: [...], edges: [...] }` to inspect neighborhood cluster size.
   - **Graph-Expanded Recall (`--multi-hop`)**:
     `lmem recall "<query>" --multi-hop` executes hybrid search and automatically traverses graph edges to pull in similar connected neighbors.

---

## 5. Choosing Between `recall`, `answer`, `related`, and `conflicts`

| Command | Output | When to Use |
| :--- | :--- | :--- |
| `lmem recall "<query>"` | Top-$K$ full memory cards with RRF score (`BM25 + Vector + Acronym Boost`), ID, category, and tags | When gathering broad context before planning, coding, or debugging |
| `lmem recall "<query>" --min-similarity 0.70` | Top-$K$ memories filtered by minimum cosine similarity threshold | When checking how many existing memories are semantically similar to a new query or statement |
| `lmem recall "<query>" --multi-hop` | Recall results expanded with 1-hop connected graph neighbors | When context requires knowing dependencies and adjacent architectural relationships |
| `lmem autolink [--min-similarity 0.75]` | Summary of newly discovered graph edges, total nodes, and edges | When auto-connecting the vault by embedding similarity, title mentions, and shared tags |
| `lmem related "<id>" -n 1` | Direct 1-hop connected neighbors, relation types, and similarity weights | When counting or inspecting memories directly linked/similar to a specific node |
| `lmem related "<id>" --hops 2` | Breadth-first connected memories along relation paths with hop-decay scores | When investigating causal chains, dependencies, or downstream impacts of a specific memory |
| `lmem graph --focus <id> -n 1` | Focused network tree or JSON subgraph for a node's immediate neighborhood | When inspecting node/edge cluster counts and topology around a concept |
| `lmem answer "<question>"` | Exact extracted span (via ONNX `minilm-squad2` / `tinyroberta-squad2` or `ollama:<model>`), confidence `%`, and source memory ID | When answering a specific factual question (`"what port..."`, `"what is my..."`, `"who is..."`) |
| `lmem conflicts` | Pairs of semantically similar + lexically overlapping memories with `[OLDER · UTC]` and `[NEWER · UTC]` timestamps | When cleaning up duplicate memories or merging updated facts into a single canonical record |

---

## 6. Best Practices for AI Agents

1. **Write Atomic, Declarative Facts**:
   - **Bad**: `"User asked me to fix the Windows installer and I found out stderr caused an error"`
   - **Good**: `"Windows PowerShell $ErrorActionPreference='Stop' treats native command stderr progress bars as terminating errors; wrap stderr-emitting CLI calls with 'Continue'."`
2. **Use `--supersede` When Updating a Fact**:
   - Passing `--supersede` on `lmem remember` automatically checks for conflicting active memories and retires the outdated record in the same transaction.
3. **Pass `--json` for Programmatic Parsing**:
   - `lmem recall`, `lmem answer`, `lmem list`, `lmem get`, `lmem stats`, and `lmem conflicts` all support `--json` with clean, deterministic schemas.
4. **Scope Project vs Global Memories**:
   - By default, `lmem` uses `./.lightmem.db` if present in the current directory or git root, falling back to `~/.lightmem/memories.db`. Pass `-g` (`--global`) to force the global vault or `--db <PATH>` for an isolated vault.
