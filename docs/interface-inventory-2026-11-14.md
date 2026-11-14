# LightMem: existing interfaces, settings, and screen inventory

Review baseline: 14 November 2026, version 0.8.0 in the current working tree.

This is an inventory of the implementation already in this checkout, including uncommitted work. It is not a proposal for a new interface. The Markdown drawings are simplified reconstructions from source, not captured screenshots or evidence of a live session. Values in angle brackets are examples/placeholders. A command result is labeled as an output, not presented as an existing interactive form.

No application code, configuration, vault data, or agent integration was changed for this review.

## 1. Interface map

```text
LightMem
├── Terminal CLI: lmem
│   ├── Welcome/status output and command help
│   ├── Guided setup + configuration preview
│   ├── Memory, attachment, search, answer, graph operations
│   ├── Identity, scope, vault and agent connection management
│   ├── Import/export, backups, migrations and maintenance
│   └── Server lifecycle, UI/docs launchers and shell completions
├── Local browser workspace: lmem ui
│   ├── Access gate + persistent vault/scope selector
│   ├── Overview
│   ├── Memories
│   ├── Connections
│   ├── Models & setup
│   ├── Backups
│   └── Settings
├── Website / embedded documentation: lmem docs
│   ├── Landing page
│   └── Searchable documentation and reference
├── Agent skill bindings: lmem connect
├── MCP stdio server: lmem mcp
├── HTTP backend: lmem serve
├── Python SDK + Python CLI launcher
├── Public Rust library
└── Shell / PowerShell installers and uninstallers
```

The terminal does not currently have a persistent, navigable full-screen application. Its welcome screen prints commands. Its interactive flows are prompts for setup, migration, observation review, conflict review, and uninstall. Shell completions and the shell installer also offer navigation, but they are separate from a full-screen terminal editor.

Primary sources: [command definitions](../src/main.rs), [terminal rendering](../src/views/cli_view.rs), [workspace HTML](../static/workspace.html), [workspace behavior](../static/workspace.js), [workspace backend](../src/services/workspace.rs).

## 2. All saved application settings

`LightMemConfig` has **9 saved fields**. They live in `<configuration root>/config.json`. The default configuration root is `~/.lightmem`; `LIGHTMEM_CONFIG_DIR` changes it. These settings are shared across vaults under that root, rather than saved independently in each vault.

| Saved field | Purpose and default | Existing editing entry point |
| --- | --- | --- |
| `backend` | Search embedding engine: `onnx` by default; also `ollama`, `hash` | `config --backend`; workspace Models & setup |
| `onnx_model` | Default `Xenova/bge-small-en-v1.5`; named options `bge-small`, `minilm`, `nomic`; CLI also accepts a custom model directory | `config --onnx-model`; workspace named-model selector |
| `ollama_url` | LLM/embedding endpoint, default `http://localhost:11434`; normalized to a base URL | `config --url`; setup custom endpoint; workspace advanced fields |
| `ollama_api_key` | Optional credential for the model endpoint; absent by default | `config --ollama-api-key`; workspace new-key/remove-key controls |
| `embedding_model` | Ollama embedding model, default `nomic-embed-text` | `config --model`; workspace advanced fields |
| `reranker` | Answer/reranking engine, default `minilm-squad2` | `config --reranker`; `setup --model`; workspace answer controls |
| `entity_extraction` | `off` by default; `llm` sends new remembered text to the configured endpoint | `config --entity-extraction`; setup; workspace |
| `remote_url` | Optional remote LightMem vault server; absent by default | `config --remote` / `--remote-url`; `--clear-remote` |
| `api_key` | Optional credential for the remote LightMem server; absent by default | `config --api-key`; `--clear-remote` |

`remote_url` and `api_key` configure access to a remote LightMem vault server. They are distinct from the model endpoint and its credential, and have no browser routing editor today.

### Presets already implemented

| Preset | Search | Answers | Entity extraction applied by preset |
| --- | --- | --- | --- |
| `demo` | Hash | `top1` | Off |
| `local` | ONNX BGE small | MiniLM SQuAD2 | Off |
| `ollama-local` | ONNX BGE small | Ollama auto | LLM |
| `ollama-cloud` | ONNX BGE small | `ollama:gemma4:31b-cloud` | LLM |

Aliases: `hash` → demo; `offline`/`onnx` → local; `cloud` → ollama-cloud; `ollama` → ollama-local. Presets modify selected fields; they do not reset the complete configuration or clear remote routing. The two Ollama presets set the endpoint to localhost. The cloud model can run through the local Ollama service. The interactive setup separately asks about entity extraction and can turn it off.

Existing answer choices: `minilm-squad2`, `tinyroberta-squad2`, `top1`, `ollama`, `ollama:<model>`. Normalization also recognizes legacy spellings such as `onnx`, `qa`, `precision`, and `openai:`/`llm:` prefixes; they are not distinct saved providers.

`setup --model` sets the answer engine. `config --model` sets the Ollama embedding model. The same flag name currently means different things in these two commands.

### Configuration operations that are not saved fields

| Operation | Existing control |
| --- | --- |
| Preview changes and selected-vault migration | `config --dry-run`, `setup --dry-run`; workspace preview dialog |
| Apply a preset | `config --preset` (alias `--profile`), `setup --preset`; workspace preset selector |
| Download models | `config --download bge-small|minilm|nomic|minilm-squad2|tinyroberta-squad2|qa|all`; workspace selected-model download buttons |
| Test saved search backend | `config --test`; workspace Test saved setup |
| Rebuild a vault's embeddings | `reindex [--yes]`; workspace Reindex this vault |
| Authorize CLI model migration | `config --yes`, `reindex --yes`; setup confirmation |
| Delete selected local vault through config | `config --reset-db --yes` |
| Clear stored remote routing and its key | `config --clear-remote` |
| Inspect configuration as JSON | `config --json`, with credentials redacted |

Changing embeddings can require reindexing the selected vault. Other vaults are not reindexed by the same action. A backend test checks embeddings; it does not verify answer quality or remote-vault health.

Source: [configuration model](../src/models/config.rs), [CLI config/setup handling](../src/main.rs), [workspace configuration patch](../src/services/workspace.rs).

## 3. Database, identity, and environment configuration

### Selection and attribution

| Control | Meaning |
| --- | --- |
| `--db PATH` | Explicit SQLite vault |
| `LIGHTMEM_DB` | Environment-selected vault; CLI `--db` takes precedence |
| `-g` / `--global` | Select the configuration root's `memories.db` when no explicit/environment DB wins |
| Existing `.lightmem.db` | Automatic current/ancestor project selection, stopping at the Git boundary |
| `--connection ID` / `LIGHTMEM_CONNECTION` | Select registered user/agent/harness/project binding and its vault |
| `--scope user|project|agent` | Override selected connection's scope |
| `--session ID` | Attribute new memories to a source session |
| `--remote URL` | One-command remote LightMem backend |
| `--api-key KEY` | One-command remote backend credential |

`--scope` and `--session` require a connection. A named connection cannot be combined with `--db`, `LIGHTMEM_DB`, or `--global`. Explicit `--remote` cannot be combined with these local DB selectors either. Without explicit remote, choosing a local database/global mode bypasses saved/environment remote routing. Remote vault operations do not support local named connections/scopes.

### Identity records already supported

```text
Configuration root
├── config.json                  shared model / routing settings
├── identities.db               users, agents, harnesses, connections
├── memories.db                 default global vault
├── models/                     model cache
└── vaults/<hashed-user>/
    ├── user.db                 user scope
    ├── projects/<hash>.db      project scope
    └── agents/<hashed-agent>/
        ├── agent.db           agent scope, no project
        └── projects/<hash>.db agent scope within a project
```

Users have ID and display name. Agents have ID, display name, and owning user. Harnesses have ID, display name, and optional custom skill path. Connections have ID, user, agent, harness, optional project, and memory scope. Session is a runtime attribution value, not a saved connection setting. These bindings organize data; they are not user authentication accounts.

### Environment controls read directly by the application

| Variable | Role / precedence |
| --- | --- |
| `LIGHTMEM_CONFIG_DIR` | Configuration, identity, global-vault and default model-cache root |
| `LIGHTMEM_DB` | Explicit vault location |
| `LIGHTMEM_CONNECTION` | Default named connection; explicit CLI connection wins |
| `LIGHTMEM_REMOTE_URL`, then `LIGHTMEM_URL` | Remote URL before saved `remote_url`; explicit CLI remote wins |
| `LIGHTMEM_API_KEY`, then `LIGHTMEM_KEY` | Remote credential before saved `api_key`; explicit CLI key wins |
| `OLLAMA_API_KEY`, then `LMEM_OLLAMA_API_KEY` | Model credential fallback after a nonempty stored `ollama_api_key` |
| `LIGHTMEM_OFFLINE` | Presence enables the application's offline policy; cached local assets are needed |
| `LIGHTMEM_QA_DISABLE` | Presence affects QA/extraction fallback behavior |
| `LIGHTMEM_QA_MODEL` | Fallback ONNX QA model spec where no explicit model spec is supplied |
| `HF_HOME` | Used when resolving the ONNX offline Hugging Face cache |
| `LMEM_BINARY_PATH`, then `LIGHTMEM_BINARY_PATH` | Python wrapper's native binary override |

Clearing a saved remote key or URL does not clear an environment value. Clearing a stored model key can expose the environment fallback. The presence-based offline/QA switches are not ordinary true/false JSON settings.

The installer additionally reads `LIGHTMEM_VERSION`, `LIGHTMEM_INSTALL_DIR` (shell), `LIGHTMEM_FROM_SOURCE` (shell), `LIGHTMEM_SKIP_MODELS`, `LIGHTMEM_NO_MODELS`, `LIGHTMEM_MODEL`, `LIGHTMEM_LLM`, and `LIGHTMEM_MCP` (shell). Platform variables such as `PATH`, `APPDATA`, `SHELL`, `PREFIX`, and `PROCESSOR_ARCHITECTURE` also affect discovery/install behavior; they are not LightMem preferences. Dependency-library environment options are outside this app-level inventory.

Source: [selection in main](../src/main.rs), [identity model](../src/models/identity.rs), [Python binary discovery](../python/src/lmem/binary.py), [install.sh](../install.sh), [install.ps1](../install.ps1).

## 4. Every CLI command and its user-facing options

There are **34 declared top-level commands**, plus generated help, global flags, and no-subcommand behavior. `mcp` is feature-gated and enabled by the default build. `identity` has 7 actions; `bundle` has 3.

This table lists canonical long options; short forms and aliases follow it. `--json` is supported by each command marked “JSON”, except export, where `--json` selects the backup format and `--output-json` selects the operation summary. The top-level `--json` is not declared a global flag, so it should not be assumed to replace each command's own JSON flag.

| Command | Inputs / options besides its JSON switch | Output mode |
| --- | --- | --- |
| `status` | No additional command fields | JSON / vault overview |
| `identity` | `user-add ID --name`; `agent-add ID --user --name`; `harness-add ID --path --name`; `connection-add ID --user --agent --harness --project --memory-scope`; `list [KIND]`; `show ID`; `status` | JSON; status also formatted |
| `context` | No additional command fields | JSON / selected context |
| `remember` | `[CONTENT]`, `--file` (repeatable), `--extract`, `--reranker`, `--type`, `--title`, `--tags`, `--confidence`, `--ttl`, `--supersede`, `--provenance`, `--no-entities` | JSON / receipt |
| `recall` | `[QUERY]`, `--image`, `--type`, `--media`, `--as-of`, `--date`, `--limit`, `--min-similarity`, `--precision`, `--multi-hop`, `--save-to`, `--path`, `--open` | JSON / ranked results / file paths |
| `list` | `--type`, `--media`, `--status`, `--as-of`, `--date`, `--limit`, `--offset`, `--page`, `--save-to`, `--path` | JSON / paginated results / file paths |
| `inspect` | `TERMS...`, `--save-to`, `--path`, `--open` | JSON / details / file paths |
| `deduplicate` | No additional command fields | JSON / merged count |
| `forget` | `[ID]`, `--all`, `--hard` | JSON / lifecycle receipt |
| `restore` | `TERM` | JSON / restored record |
| `clear` | `--soft` | JSON / clearing receipt |
| `export` | `--okf`, `--json`, `--bundle`, `--output`, `--output-json` | Files / optional JSON summary |
| `import` | `FILE`, `--enrich`, `--replace`, `--merge` | JSON / import receipt |
| `bundle` | `export --output`; `import FILE --replace --merge`; `info FILE` | JSON / bundle receipts / manifest |
| `answer` | `QUESTION`, `--precision`, `--reranker`, `--type`, `--as-of`, `--date`, `--limit` | JSON / answer + source |
| `config` | `--backend`, `--onnx-model`, `--download`, `--url`, `--preset`, `--model`, `--reranker`, `--entity-extraction`, `--ollama-api-key`, `--dry-run`, `--test`, `--reset-db`, `--yes`, `--remote`, `--api-key`, `--clear-remote` | JSON / settings / preview / diagnostics |
| `setup` | `--preset`, `--url`, `--model`, `--entity-extraction`, `--dry-run`, `--yes` | JSON / guided prompts / preview |
| `reindex` | `--yes` | JSON / migration progress |
| `stats` | No additional command fields | JSON / storage statistics |
| `conflicts` | `--yes`, `--reranker`, `--min-similarity` | JSON / report / interactive merge review |
| `link` | `SOURCE TARGET`, `--relation`, `--weight` | JSON / link receipt |
| `unlink` | `SOURCE TARGET`, `--relation` | JSON / unlink receipt |
| `related` | `ID`, `--hops` | JSON / connected memories |
| `graph` | `--focus`, `--hops` | JSON / graph tree |
| `autolink` | `--min-similarity` | JSON / new links summary |
| `observe` | `[INPUT]`, `--file`, `--yes`, `--dry-run`, `--type`, `--tags`, `--confidence`, `--reranker`; also stdin | JSON / candidates / interactive save review |
| `connect` | `[PLATFORM]`, `--workspace`, `--path`, `--list`, `--mcp` | JSON / detection / install result |
| `mcp` | No command-specific fields; global selection applies | Stdio JSON-RPC server |
| `completions` | `SHELL`: zsh, bash, fish, elvish, powershell | Completion script |
| `version` | No additional command fields | JSON / version metadata |
| `docs` | `--port`, `--landing`, `--no-browser` | Local docs server and browser launch |
| `ui` | `--port`, `--no-browser` | Local authenticated workspace and browser launch |
| `serve` | `--host`, `--port`, `--api-key`, `--no-auth`, `--daemon`, `--stop`, `--status` | Foreground/background backend lifecycle |
| `uninstall` | `--purge-data`, `--yes` | JSON / uninstall prompt and output |

Global selection flags: `-g/--global`, `--connection`, `--scope`, `--session`, `--db`, `--remote`, `--api-key`. Top-level information flags: `-V/--version`, `--json`; generated help uses `-h/--help`.

Aliases: `vaults` → status; `get`/`show` → inspect; `dedup` → deduplicate; `purge`/`reset` → clear; `auto-link` → autolink; `observed`/`extract` → observe; `doc`/`web` → docs; `server` → serve. Option aliases include `--attach` → `--file`, `--img` → `--image`, `--paths` → `--path`, `--export-attachments` → inspect's `--save-to`, `--no-extract-entities` → `--no-entities`, `--profile` → preset, `--remote-url` → config remote, `--print-json` → output-json, `--resolve` → yes for conflicts/observe.

Common short forms: `-t` type, `-m` media, `-l` limit, `-f` file (or graph focus), `-r` reranker (or link relation), `-c` confidence, `-s` supersede, `-o` output/save-to, `-p` path (or remember provenance, docs/serve port, connect destination), `-n` hops, `-w` link weight or connect workspace, `-y` yes where declared, `-d` daemon, `-a` uninstall purge-data. Short names depend on the command; they are not universal settings.

Important implementation boundary: `import --enrich` is declared but currently rejected as unsupported by the controller. It is not an available enrichment workflow.

## 5. Existing terminal screens and prompt flows

Each `T` ID identifies a reviewable output/prompt family. These drawings describe existing behavior; brackets are fields or choices only where the implementation actually prompts.

### T01 — Welcome and help

```text
$ lmem
  LIGHTMEM v0.8.0
  Vault   <selected database>
  Status  <active count> · <vector count>
  Engines <embedding engine> + <answer engine>
  QUICK COMMANDS
  lmem setup | lmem config --test | lmem context
  remember | recall | answer | graph | related | list | connect | stats

$ lmem --help                 $ lmem <command> --help
  Usage / commands / options   Usage / arguments / command options
```

This is printed guidance, not a selectable dashboard.

### T02 — Guided setup and custom endpoint

```text
$ lmem setup
  Settings are shared across vaults in <configuration root>.
  1. Local ONNX — search + extractive answers; downloads needed
  2. Demo — hash search + top memory; no downloads
  3. Local Ollama — ONNX search + local LLM answers
  4. Ollama cloud model — ONNX search + cloud answers via localhost
  5. Custom LLM — enter endpoint and model; ONNX search
  Choose [1-5, default 1]: _
       |
       +-- if Custom: LLM base URL [localhost:11434]: _
       |              LLM model name (required): _
       +-- if LLM entities enabled:
           Send remembered text to <endpoint> for entity extraction? [y/N] _
       |
       v
  Configuration preview (T03)
  Save shared settings and approve selected-vault migration? [y/N] _
```

### T03 — Configuration display and preview

```text
$ lmem config
  LightMem Configuration
  Shared settings / entity extraction / offline policy / key presence
  ├─ Backend engine       <onnx|ollama|hash>
  ├─ Active embedder      <model>
  ├─ Embedding identity   <identity>
  ├─ Active reranker      <engine/model>
  ├─ Endpoint / model details
  └─ Config and database paths / preset guidance

$ lmem config <changes> --dry-run
  Configuration preview — no changes applied
  Config file / shared scope / selected vault
  Search / answers / entity destination / saved remote
  Changed fields: <fields>
  Selected-vault index: <migration needed or not>
  Other vaults are not reindexed.
```

There is no terminal form to edit a highlighted setting. Changes use flags or the setup prompts.

### T04 — Index migration, download, and diagnostics

```text
$ lmem reindex
  Embedding migration for <vault> (<count> memories)
  From: <stored identity>
  To:   <requested identity>
  Rebuild embeddings now? ... [y/N] _
  Embedding progress: <done>/<total> (<percent>%)
  Migration complete: <count> embeddings committed.

$ lmem config --download <model|all>
  <model initialization/download output>

$ lmem config --test
  Embedding test passed: <dimensions> (<backend>). No memory saved.
  Answer quality is not tested; use answer with a known fact.
```

These are distinct actions. A download does not itself provide a full readiness test.

### T05 — Vault overview, context, and identities

```text
$ lmem status                # also: vaults / identity status
  Default vault: <path, existence, active/total>
  Connections:
  <connection> <harness> <user/agent/project> <scope> <vault/counts>

$ lmem context
  <local selected vault and identity/scope/session>
  OR remote endpoint / online status / auth presence / remote counts

$ lmem identity <user-add|agent-add|harness-add|connection-add|list|show>
  { <created, listed, or resolved identity data> }
```

Identity registration is argument-driven; there is no multi-step terminal connection form.

### T06 — Remember receipt

```text
$ lmem remember <content> [metadata, files, TTL, extraction options]
  <saved category/title/content/ID>
  <tags, provenance, confidence, attachment details as applicable>
  <conflict/merge information when applicable>
  <vault context>
```

Inputs: content; one/multiple files; optional LLM attachment extraction; model override; category; title; tags; confidence (default 0.9); TTL; supersede; provenance; skip entities. There is no content editor prompt.

### T07 — Recall, chronological list, and memory inspection

```text
$ lmem recall <query> [filters]
  Ranked memories: <rank/score> <title/type/ID/content>
  <attachment details and graph expansion when requested>

$ lmem list [filters] --page <n>
  <chronological memory rows>
  Page <n>/<total pages> · <total records> · <pagination details>

$ lmem inspect <ID, prefix, or title> [more terms...]
  Title / full content / metadata / timestamps
  Attachments / entities / relationships
```

Recall/list/inspect can export or materialize attachments. Recall/inspect can open them in the system application. `--path` prints file paths for scripting. Image query is a recall option. A path result or opened attachment is not an edit screen.

### T08 — Answer

```text
$ lmem answer <question> [filters, engine override]
  <answer>
  <answer source / engine / scoring information>
```

Options include precision, reranker override, type, as-of date, calendar date, and candidate limit. An exact entity-index match can supply an answer without the answer model.

### T09 — Observe candidate review

```text
$ lmem observe <text, transcript path, --file, or stdin>
  <extracted candidate list>
  [Candidate <n>/<total>] [<category>] <title> (<confidence>) [<provenance>]
    Content: <candidate text>
  Save this memory? [y]es / [n]o (skip) / [a]ll / [q]uit: _
  <saved/skipped totals>
```

`--dry-run` displays candidates; `--yes` saves all. JSON or noninteractive input without `--yes` returns candidates without saving. Candidate review supports accept/skip, not inline editing of the candidate.

### T10 — Conflict review and deduplication

```text
$ lmem conflicts
  Pair <n>/<total> · <similarity>
  Older memory: <content>
  Newer memory: <content>
  Merge these memories? [y]es / [n]o (next) / [a]ll / [q]uit: _
  <merge result / skipped pair / totals>

$ lmem deduplicate
  <number of merged duplicates>
```

Conflict threshold defaults to 0.78. Conflict `--yes` resolves in batch; JSON without `--yes` reports. Deduplication is its own direct mutation, not an interactive per-pair review.

### T11 — Archive, restore, delete, and clear outputs

```text
$ lmem forget <id> [--hard]   OR forget --all [--hard]
  <retired or permanently deleted result>
$ lmem restore <term>
  <restored memory record; old TTL cleared>
$ lmem clear [--soft]
  <removed count / deletion mode / vault>
$ lmem config --reset-db --yes
  <selected local vault deletion result>
```

These are command outputs, not a shared terminal confirmation dialog. `clear` defaults to permanent clearing; `--soft` retires instead.

### T12 — Graph and relationship operations

```text
$ lmem graph [--focus <memory>] [--hops <n>]
  <memory>
  ├── <relation> → <linked memory>
  └── <relation> → <linked memory>

$ lmem related <memory> --hops <n>
  <connected memory rows / distance and score information>

$ lmem link <source> <target> --relation <name> --weight <value>
$ lmem unlink <source> <target> [--relation <name>]
$ lmem autolink --min-similarity <threshold>
  <operation receipt / link count>
```

Related/focused graph default to 2 hops. Link relation defaults to `relates_to`, weight to 1.0. Autolink threshold defaults to 0.75. There is no interactive terminal graph editor.

### T13 — Import, export, and bundle results

```text
$ lmem export [--okf|--json|--bundle] --output <destination>
  <exported path / format>
$ lmem import <file or directory> [--replace|--merge]
  <source / imported record count>

$ lmem bundle info <bundle>
  <manifest: vault, record, media, and embedding information>
$ lmem bundle export --output <bundle>
  <bundle destination / summary>
$ lmem bundle import <bundle> [--replace|--merge]
  <restored/merged result>
```

Import sources include structured JSON/JSONL, OKF files/directories, supported graph/memory representations, and transfer ZIP bundles. The CLI uses arguments for destination/mode; it does not have the browser's restore preview form.

### T14 — Agent detection and integration results

```text
$ lmem connect --list
  Platform      Detected?      Target skill location
  Antigravity   <yes/no>       <path>
  Codex         <yes/no>       <path>
  Hermes        <yes/no>       <path>
  Cursor        <yes/no>       <path>
  Claude        <yes/no>       <path>
  Agents        <yes/no>       <path>

$ lmem [--connection <id>] connect [platform|all] [--workspace|--path]
  <per-target installed binding / destination / result>
$ lmem [--connection <id>] connect [platform] --mcp
  <per-target MCP configuration result>
```

Skill deployment supports the six platforms above. Automated MCP configuration targets Claude, Cursor, and Antigravity. A detected/installed integration is not proof that an agent has successfully saved and recalled a memory.

### T15 — Stats, server lifecycle, launchers, and developer-facing output

```text
$ lmem stats
  Vault <path> · total / active / expired / vectors / category counts

$ lmem serve [--host <host>] [--port <port>] [--daemon]
  <address / authentication setup / client setup / server output>
$ lmem serve --status        <background daemon status>
$ lmem serve --stop          <stop result>

$ lmem ui [--port <port>] [--no-browser]
  <private local workspace URL; loopback only>
$ lmem docs [--port <port>] [--landing] [--no-browser]
  <local documentation URL>

$ lmem version / lmem --version
  <version and build metadata>
$ lmem completions <shell>   <generated shell script>
$ lmem mcp                   <stdio protocol, not a human screen>
```

`serve` defaults to `0.0.0.0:8080`, supports an explicit API key or generates one, and offers `--no-auth`. Docs normally auto-select a port in 8080–8100. Workspace is a separate local management launch mode.

### T16 — Native uninstaller

```text
$ lmem uninstall [--purge-data]
  Removing LightMem engine, binaries & shell completions
  Proceed with uninstalling LightMem? [y/N]: _
  <cleanup result>
```

`--yes` bypasses its prompt. Data removal is separately controlled by `--purge-data`. Shell and PowerShell uninstall scripts also exist. There is no browser uninstall page.

## 6. Existing local browser workspace screens

The workspace has six main navigation pages, an access gate, persistent context controls, three dedicated form/review dialogs, and a reusable confirmation dialog. Drawings below separate detail, preview, and progress states so each can be reviewed.

### W01 — Access gate

```text
+-----------------------------------------------------------+
| Welcome to your workspace.                                |
| Open the private URL printed by lmem ui, or enter the key. |
| Workspace API key [***********************************]   |
| [Open workspace →]                                        |
| <access error, if any>                                    |
+-----------------------------------------------------------+
```

The private launch URL can supply access automatically. The key is held in the tab session; “Lock this tab” returns to the gate.

### W02 — Persistent workspace frame and vault selection

```text
+-------------------+-----------------------------------------+
| LightMem  LOCAL   | TARGET: This computer                    |
|                   | VAULT / CONNECTION [<choice> v]          |
| Overview          | SCOPE [user|project|agent v] [Refresh]    |
| Memories          +-----------------------------------------+
| Connections       | <page title and description>             |
| Models & setup    |                         [+ Add memory]  |
| Backups           | <notice / operation progress if needed>  |
| Settings          |                                         |
|                   | <selected page>                         |
| Running locally   |                                         |
| <version>         |                                         |
| [Lock this tab]   | <selected vault footer>                 |
+-------------------+-----------------------------------------+
```

The vault selector offers the initial vault and registered connections. Scope applies to a connection. Selecting arbitrary DB paths or another configuration root requires restarting the workspace with CLI/environment choices.

### W03 — Overview

```text
+------------------------------------------------------------+
| Your memory, at a glance.                    [+ Add memory] |
| [Active] [Indexed vectors] [Named connections] [Archived]    |
+-------------------------------+----------------------------+
| Setup checklist               | Your current setup         |
| <four readiness steps>        | Selected vault             |
| [View / Set up]               | Find memories / answers    |
|                               | Entity extraction          |
|                               | [Configure your setup →]   |
+-------------------------------+----------------------------+
| Recent memories                              [View all →]  |
| <memory rows / empty state>                                |
+------------------------------------------------------------+
```

Connecting a tool does not automatically record its conversations; the overview explains this boundary.

### W04 — Memories: browse, search, answer, and filters

```text
+------------------------------------------------------------+
| [Find a decision, fact, tag, or file…] [Search|Ask v] [Go]   |
| Category [all v] Media [all v] Status [active|archived v]    |
| UTC date [yyyy-mm-dd]                       [Clear filters] |
| <answer + engine + View source memory, in answer mode>      |
+-------------------------------+----------------------------+
| Memories <result count>       | Memory details             |
| <title/type/tags/preview>      | <selection or empty state> |
| <additional rows>             |                            |
| [Previous]  Page <n>  [Next]   |                            |
+-------------------------------+----------------------------+
```

Browser media choices: images, documents, audio, video, any attachment, or all memories. CLI additionally exposes code/archive/binary filters. Search/answers use active memories; clear the search to browse archived ones. There is no browser as-of, image-query, precision, or multi-hop control.

### W05 — Memory detail and attachment actions

```text
+------------------------------------------------------------+
| <category> · Active / Archived                             |
| <title>                                                    |
| <full content>                                             |
| #tags                                                      |
| Created / updated / provenance / connection / scope         |
| Session / expiry when present                              |
| ATTACHMENTS   <filename, MIME, size> [Download]              |
| EXTRACTED ENTITIES   <name → value>                         |
| RELATIONSHIPS  <relation → linked ID> [Open linked memory]  |
| [Copy memory ID] [Archive memory OR Restore memory]         |
+------------------------------------------------------------+
```

Archive/restore use the reusable confirmation dialog. There is no Edit button, attachment-replacement form, permanent-delete button, or relationship editing control here.

### W06 — Add memory dialog

```text
+------------------------------------------------------------+
| Add a memory                                           [×] |
| <write destination>                                        |
| Title (optional) [.......................................] |
| Memory           [multiline content......................] |
| Category [automatic v]  Tags [comma-separated............] |
| Attachments [Choose files]                                 |
| > Advanced                                                 |
|   Confidence [0.9]     Expires after [7d / 30m............] |
|   [x] Skip LLM entity extraction for this memory           |
| <validation error>                      [Cancel] [Save]    |
+------------------------------------------------------------+
```

The browser has no controls for supersede, explicit provenance, or LLM extraction of attached file contents. The CLI has those options. The browser's skip-entities box starts checked.

### W07 — Connections and supported tools

```text
+------------------------------------------------------------+
| Give each agent a clear connection. [+ Create connection]   |
| Verify a working memory cycle: reload tool; save; recall.   |
| Your connections                                           |
| <ID> <tool> <user/agent> <project/scope> <vault path>        |
| <generated context test command>                          |
| [Use vault] [Copy test command] [Install skill]             |
| [Configure MCP, for supported tools]                       |
| Supported tools: <detection status and target paths>       |
+------------------------------------------------------------+
```

### W08 — Create connection dialog

```text
+------------------------------------------------------------+
| Create a named connection                              [×] |
| Connection ID [..........................................] |
| User ID [................] Agent ID [.....................] |
| Tool [Codex|Claude|Cursor|Antigravity|Hermes v]              |
| Scope [project|user|agent v] Project ID [.................] |
| <validation error>                  [Cancel] [Create]      |
+------------------------------------------------------------+
```

Default scope here is project; CLI `identity connection-add` defaults to user. The browser creates/registers the user and agent as part of connection creation. Display names and arbitrary custom harness paths have no browser form. The generic `agents` target exists in CLI/registry but is absent from this creation dropdown.

### W09 — Installation and lifecycle confirmation dialog

```text
+------------------------------------------------------------+
| <Install skill? / Configure MCP? / Archive? / Restore?>     |
| <specific tool, destination, connection, or memory details> |
| <error if the action fails>                                |
|                                      [Cancel] [Confirm]   |
+------------------------------------------------------------+
```

This is one reusable dialog. It also confirms reindexing. Connection records have no browser rename/edit/delete dialog.

### W10 — Models & setup

```text
+-----------------------------------+------------------------+
| Choose how memory works.          | Models & readiness     |
| Preset [custom/local/Ollama/demo]  | <last search test>     |
| Find memories [ONNX/Ollama/hash]   | [Download embeddings]  |
| Local model [BGE/MiniLM/Nomic]     | [Download answer model]|
| Produce answers [MiniLM/          | [Test saved setup]     |
|   TinyRoBERTa/top1/Ollama]         | [Reindex this vault]   |
| Extract entities [off/LLM]        +------------------------+
| > Endpoint and advanced settings | Where data goes        |
|   Endpoint URL [...............]  | storage/search/answer/ |
|   Ollama embedding model [....]  | extraction boundaries  |
|   Ollama answer model [.......]  |                        |
|   New API key [***************]  |                        |
|   [ ] Remove stored Ollama key   |                        |
| [Preview changes →]              |                        |
+-----------------------------------+------------------------+
```

The model endpoint credential is editable here. Remote LightMem URL/key are separate CLI settings. A custom ONNX model directory is supported through the CLI, not this named-model selector.

### W11 — Configuration review dialog

```text
+------------------------------------------------------------+
| Review your configuration                              [×] |
| Embedding backend / local model / answer engine            |
| Entity extraction / LLM destination / vault to migrate      |
| <record count and migration explanation>                   |
| [ ] Reindex this vault with new embedding settings         |
| Settings shared under this configuration root.             |
| <error>                    [Cancel] [Apply reviewed settings]|
+------------------------------------------------------------+
```

The reindex consent control appears when needed. Applying is an asynchronous job. External configuration edits require restarting the workspace before applying changes.

### W12 — Progress and exceptional states

```text
+------------------------------------------------------------+
| <notice: stale config / index mismatch / local-vs-remote>    |
| <operation: model download / configuration / reindex /     |
|             backup restore>                               |
| <message>  [================...........] <progress>        |
| <completion or failure message>                            |
+------------------------------------------------------------+
```

Writes are disabled while a tracked job is busy. Empty memory/connection lists, no search results, API errors, validation errors, test results, copy/download feedback, and access failures also have existing UI states. There is no user-facing job history or cancel-job control.

### W13 — Backups and bundle inspection

```text
+-------------------------------+----------------------------+
| One portable bundle.          | Bring a vault back.        |
| Vault + vectors + links       | Bundle [Choose ZIP file]   |
| + attached files             | Up to 40 MiB here          |
| [Download backup]             | [Inspect bundle]           |
| <download time this session>  |                            |
+-------------------------------+----------------------------+
```

### W14 — Restore preview and confirmation

```text
+------------------------------------------------------------+
| Restore preview                                            |
| <manifest summary / destination / current record count>     |
| Restore mode [Merge into this vault | Replace this vault v] |
| <mode explanation>                                         |
| If replacing: Type REPLACE [..............................] |
| [Restore reviewed bundle]                                  |
| <restore job / result>                                     |
+------------------------------------------------------------+
```

The selected vault and its revision are checked against the preview. Manifest inspection alone is not full archive validation. Browser backup handling is bundle-oriented; general JSON/OKF import/export remains in the CLI/library.

### W15 — Settings (informational)

```text
+------------------------------------------------------------+
| Your effective workspace.                                  |
| Target / vault path / connection / scope                    |
| Configuration file / model cache / storage protection       |
| Offline override / QA-disabled override                     |
| Remote endpoint configured for CLI?                        |
| Restart command guidance for another root/database         |
| Configuration precedence explanation                       |
+------------------------------------------------------------+
```

This page displays effective settings and explanations. It does not have editable fields or a Save button. Models & setup is the browser's existing configuration editor.

## 7. Installer, documentation, and website screens

### I01 — Shell installer model selector

```text
Select ONNX Embedding Model(s) to Download
  > [1] bge-small  · 384 dimensions
    [2] minilm     · 384 dimensions
    [3] nomic      · 768 dimensions
    [4] all        · download all three
    [5] none       · skip local model downloads
Use ↑/↓ or 1–5, then Enter
```

The shell installer also prewarms default QA when models are not skipped. PowerShell currently pre-caches BGE small and MiniLM QA or skips; it does not expose this five-choice model menu.

### I02 — Installer LLM selector and follow-up prompts

```text
Configure LLM Engine for Memory Ingestion & QA
  > [1] Local Ollama
    [2] Ollama Cloud
    [3] Custom remote LLM endpoint
    [4] Offline local ONNX
       |
       +-- cloud: model name
       +-- custom: endpoint URL + model name
       +-- status: local Ollama detected / start guidance
```

The shell uses arrow/number selection; PowerShell uses numbered prompts. This menu defaults to Local Ollama, whereas `lmem setup` defaults to Local ONNX. Its wording about offline/cloud should not be mistaken for the effective routing/environment policy; local preset application does not clear a saved remote URL or enforce `LIGHTMEM_OFFLINE`.

### I03 — Installer integration and completion states

```text
<binary download/verification or source build progress>
<model caching and configuration output>
<shell completion installation, on shell installer>
<agent skill deployment result>
Configure MCP in detected client configs? [y/N] _
<MCP result>
LightMem installation complete! Run: lmem
```

Skip-model flags on shell: `--skip-models`, `--no-models`, `--skip-local-models`, `--skip-lockout-models`. PowerShell has `-SkipModels`. The shell installer also delegates `--uninstall`/`-u` to its uninstaller. These installer controls are separate from normal `lmem` commands.

### D01 — Documentation browser

```text
+--------------------------+---------------------------------+
| LightMem / Docs          | Search documentation [.......]  |
+--------------------------+---------------------------------+
| Start                    | <selected documentation section>|
|  Overview / Quick start  | Installation tabs:              |
|  Workspace / Architecture| Python / Shell / Windows / Source|
|  Data model              |                                 |
| Core engine              | Code examples [Copy]            |
|  Retrieval / Models      | Source links                    |
|  Graph / Conflicts & TTL |                                 |
| Reference                |                                 |
|  CLI / Config / Formats  |                                 |
|  Python / Integrations   |                                 |
| Operations               |                                 |
|  Distribution / Dev      |                                 |
|  Privacy / Caveats       |                                 |
+--------------------------+---------------------------------+
```

### D02 — Landing website

```text
LightMem branding and navigation
  Product introduction / install guidance
  Feature and memory-engine explanations
  Documentation / repository links
```

These are documentation/marketing surfaces, not authenticated memory-management pages. Static hosting is separate from the local workspace backend; serving static files alone does not provide local vault operations.

## 8. Programmatic interfaces without human screens

### MCP

```text
Agent client → MCP stdio server → bound LightMem controller
  context
  remember
  recall
  answer
  graph_related
  conflicts
  config
  forget
  stats
```

These **nine tools** are the currently advertised tool list. MCP config exposes a subset: backend, ONNX model, reranker, endpoint URL, preset, entity extraction. It is not the complete CLI or the complete browser workspace. MCP remember includes session attribution, metadata, files, extraction, TTL, and supersede controls. Its `extract_entities` schema description says default true; effective extraction remains subject to configured opt-in in the implementation. Do not infer a new global default from that tool description.

Source: [MCP tools and handlers](../src/services/mcp.rs).

### HTTP backend

```text
Client → authenticated HTTP server → one selected controller/vault
  GET    health, stats, links, attachment, list, get/inspect,
         related, graph
  POST   remember, recall, answer, forget, link, unlink, clear
  DELETE forget
```

Canonical routes use `/v1/...`; many core routes also have legacy unversioned aliases. Authentication accepts bearer credentials / supported API-key header. The health route is separately accessible. Workspace management routes provide status, config preview/apply, reindex, download, test, restore-memory, vault selection, connection creation/install, and bundle restore preview/apply. They belong to local management behavior and should not be assumed to be a public remote administration API.

Remote CLI routing supports remember, recall, answer, list, inspect, forget, clear, stats, link, unlink, related, graph, and remote context, with option restrictions. Attachment export/materialization/open flags are not available through remote CLI flows. Unsupported remote commands fail explicitly instead of silently operating on a local vault. Administrative commands such as config/setup/UI/docs/serve/version/completions, non-MCP connect, bundle info, and uninstall continue locally; identity/status are handled locally before remote routing.

Source: [HTTP routes](../src/services/api_server.rs), [remote CLI dispatch](../src/main.rs), [workspace routes](../src/services/workspace.rs).

### Python SDK and launcher

```text
Python LightMem(...)
  selection: db_path, global_db, connection, scope, session
       |
       v
  native lmem subprocess + JSON
```

Public methods/properties currently include: `binary_path`, `context`, `register_user`, `register_agent`, `register_harness`, `register_connection`, `identities`, `connection_info`, `vault_status`, `remember`, `recall`, `answer`, `list`, `get`, `inspect`, `forget`, `deduplicate`, `conflicts`, `link`, `unlink`, `related`, `graph`, `autolink`, `connect`, `stats`, `observe`, `bundle_export`, `bundle_import`, and `bundle_info`.

The Python CLI entry point passes arguments to the native executable; it is not a separately designed terminal UI. SDK convenience methods do not cover every newer CLI action: there is no dedicated SDK `restore`, config/setup editor, reindex method, or general import/export wrapper in the current client.

Source: [Python client](../python/src/lmem/client.py), [CLI wrapper](../python/src/lmem/cli.py).

### Rust library

The public `LightMem` / `MemoryController` API exposes controller construction/configuration, embeddings readiness and migration, memory creation, attachments, recall/answers, temporal listing, entity lookup, lifecycle operations, graph operations, import/export, bundles, statistics, observation, and repository access. It is another existing interface, but has no independent human screen. Lower-level storage access is not an existing user-facing memory editor.

Source: [exports](../src/lib.rs), [controller API](../src/controllers/memory_controller.rs).

## 9. Shared data choices that a future design must account for

These choices already exist; they are not all global settings.

| Area | Existing choices |
| --- | --- |
| Memory categories | `fact`, `decision`, `instruction`, `preference`, `learning`, `goal`, `commitment`, `artifact`, `event`, `relationship`, `observation`, `error`, `context`, `password` |
| Lifecycle | Active / expired; TTL durations such as `30s`, `15m`, `24h`, `7d`, `2w`; restore clears previous TTL |
| Memory metadata | Title, content, tags, confidence, provenance, identity/session context; generated IDs and timestamps |
| Provenance examples | `explicit_statement`, `validated`, `observed`, `corrected`, `inferred`, `imported`, `attachment_metadata`, `attachment_llm_extracted` |
| Attachments | Multiple files; metadata and stored binary; extraction opt-in; export/materialize/open; images, audio, video, documents, code, archives, binary |
| Retrieval | Text or supported image query; type/media/date/as-of filters; limits; similarity threshold; precision; graph expansion |
| Listing | Active/expired, temporal/category/media filters, limit, offset/page |
| Graph | Directional source/target/relation/weight; related hops; graph focus; autolink threshold |
| Observation | Text/file/stdin, candidate confidence (default 0.85), type/tags, model override, dry-run, per-candidate accept/skip or save-all |
| Conflict resolution | Similarity threshold, model override, report/review/merge-all; separate deduplication |
| Data portability | OKF / JSON backup / ZIP bundle; destination; inspect; merge/replace |
| Agent integration | Platform, global/workspace/custom installation location, bound connection, skill vs MCP |
| Server operation | Host/port, auth/generated key/no-auth, foreground/background, status/stop |
| Presentation | Human output or JSON where supported; file-path-only attachment output; shell completions |

Dates use the implementation's UTC temporal semantics. Browser date filtering is explicitly labeled UTC. A confidence value is memory metadata, while a recall score expresses ranking; the workspace explains that a recall score is not a probability.

## 10. What does not have an existing editor or screen

These are observations about the current implementation, not new screen proposals.

- No persistent full-screen terminal dashboard with selectable settings or memory rows.
- No dedicated user-facing edit/update command or browser form for an existing memory's title, content, tags, confidence, category, expiry, or attachments. Existing supersede/merge/import-replace paths are different workflows.
- No observation-candidate text editor; review currently accepts or skips candidates.
- No dedicated identity/connection rename/delete CLI actions or browser forms.
- No browser remote-LightMem URL/key editor or server lifecycle page.
- No browser conflict/deduplication review page, observation page, or full graph editor; memory detail shows navigable relationships.
- No browser general JSON/OKF import/export form, arbitrary database-path picker, session-attribution input, or custom ONNX-directory picker.
- No single universal precedence rule: model credentials prefer a stored key before environment fallback, while remote credentials prefer environment before saved values.
- No single setup experience across installation, CLI setup, and browser model setup; choices, defaults, and exposed fields differ.
- No persistent backup schedule, model-job history, or user-facing job cancellation controls in the workspace.

## 11. Review index

Use these IDs when reviewing the current behavior before any redesign:

```text
Terminal
T01 Welcome/help       T02 Guided setup       T03 Config/preview
T04 Models/migration   T05 Vaults/identities   T06 Remember
T07 Browse/search/get  T08 Answer             T09 Observe review
T10 Conflicts/dedup    T11 Memory lifecycle   T12 Graph/links
T13 Portability        T14 Integrations       T15 Ops/launchers/stats
T16 Uninstall

Browser workspace
W01 Access             W02 Frame/context      W03 Overview
W04 Browse/search/ask  W05 Memory detail      W06 Add memory
W07 Connections        W08 Create connection  W09 Confirmation
W10 Models/setup       W11 Config review      W12 Progress/states
W13 Backup             W14 Restore preview    W15 Settings

Installation and documentation
I01 Model selector     I02 LLM selector       I03 Install/integration
D01 Documentation      D02 Landing website
```

## 12. Verification boundary

This document was checked against the current command enums, configuration model and precedence code, terminal renderers, HTML dialogs and controls, JavaScript handlers, workspace routes, remote dispatch, MCP tool declarations, Python public client, controller methods, and installer scripts. Counts and command coverage were checked directly from source. No new application behavior was implemented, and no live-model/network/server acceptance test was performed for this inventory.
