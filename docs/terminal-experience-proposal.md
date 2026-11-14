# LightMem terminal experience: proposed changes

Superseded: the user prefers ordinary terminal commands and does not want a TUI. Use the [command-line proposal](./command-line-experience-proposal.md) for current plans. The screen designs below are retained only as the earlier proposal, not the implementation direction.

Draft for review · 14 November 2026.

This document proposes the next interface changes. All screens, new commands, and behavior below are planned, not implemented. The baseline is the [existing interface inventory](./interface-inventory-2026-11-14.md). Example counts, paths, names, and results in drawings are illustrative.

## 1. The change in everyday use

A user should be able to open LightMem, choose where their memory lives, complete a guided setup, browse and edit memories, and understand settings without learning dozens of flags.

Add an explicit interactive entry point:

```text
lmem tui                      Open the terminal workspace
lmem config --interactive     Open its settings area
lmem edit <memory>            Open a memory editor
lmem doctor                   Explain setup and readiness
```

These are proposed commands. The existing direct commands, aliases, JSON output, stdin workflows, and Python subprocess behavior remain supported. The current `lmem` welcome output gains an obvious “Open interactive workspace: lmem tui” entry. Explicit launch avoids making a full-screen interface appear in scripts or existing workflows.

The terminal workspace can open directly on a local vault or named connection using existing global selection flags. Remote selection shows the remote capabilities actually available; it must not silently change to local data when an operation is unsupported.

## 2. What changes, and why

| Today | Proposed change | User benefit |
| --- | --- | --- |
| Welcome screen prints commands | P01 navigable home with readiness and next actions | A useful starting point |
| Installer, setup, and browser use different setup experiences | P02 shared setup steps, defaults, and review rules | One explanation of how LightMem works |
| Vault/connection selection requires understanding flags | P03 visible destination and searchable vault selector | Know where an action will read/write |
| Memory inspection has no edit action | P05–P07 memory form, draft, diff, and save | Correct existing memories directly |
| Search features are spread across options | P04 filters and separate search/answer modes | Discover retrieval features in context |
| Attachment actions are flags | P08 attachment list and actions | Find, attach, export, and open files |
| Candidate review accepts or skips | P09 edit candidates before saving | Fix extracted text without restarting |
| Conflict merging has limited preview | P10 compare originals and review proposed merge | Understand what resolving a conflict will do |
| Graph operations require separate commands | P11 browsable relationships and link forms | Manage relationships from a memory |
| Identity/connection registration is fragmented | P12 connection wizard with destination and tool binding | Complete an integration in one flow |
| Model/routing/credential terms are easy to confuse | P13–P16 grouped settings, source labels, and change review | Understand each setting and its effects |
| Backend test checks only embeddings | P17 separate checks with specific results | Distinguish model readiness, remote reachability, and agent verification |
| Portability uses multiple commands | P18 guided import/export/restore | Choose the correct format and destination |
| Server lifecycle is flag-driven | P19 server status and explicit lifecycle actions | Operate a server from one screen |
| Long work provides transient output | P20 operation status with accurate progress and recovery | Understand what is happening and what to do after failure |
| Destructive utilities are mixed into general command discovery | P21 maintenance with explicit target and effect | Recognize actions that affect many records |
| Help is separate from tasks | P22 contextual help and command equivalents | Learn the CLI while using the workspace |

## 3. Proposed navigation and interaction

```text
LightMem terminal workspace
├── Home
├── Memories
│   ├── Browse / search / ask
│   ├── View / add / edit
│   └── Files / archive / restore / relationships
├── Review
│   ├── Conversation candidates
│   └── Conflicts and duplicates
├── Connections
│   └── Vaults / identities / agent skill / MCP binding
├── Settings
│   ├── Storage & routing
│   ├── Search
│   ├── Answers
│   ├── Extraction
│   ├── Credentials
│   └── Environment & paths
├── Backups & transfers
└── Tools
    ├── Diagnostics
    ├── Graph
    ├── Server
    ├── Operations
    ├── Maintenance
    └── Help
```

Every screen shows the active destination, connection/scope when present, and pending changes. The expanded destination view shows the full path or endpoint. Choosing a remote vault makes that mode prominent. Settings that administer this computer are labeled separately from operations against a remote vault.

Navigation uses arrow keys, Enter, Tab/Shift+Tab, Esc to return, and a visible contextual footer. Text-entry fields receive normal typing; navigation shortcuts do not intercept characters while editing. Forms use labeled actions and a shortcut for Save, such as Ctrl+S. Esc on a dirty form asks whether to keep editing or discard. Leaving or switching vaults does not silently apply a draft.

The standard layout targets terminals around 100 columns. Narrow terminals stack lists and detail views. Color is optional; focus, status, and errors also have text/symbol cues. Non-terminal launches report how to use the direct commands. A terminal unsupported by the full-screen renderer can use the existing line prompts and direct CLI.

## 4. Screen drawings

### P01 — Home

```text
┌─ LightMem ─────────────────────────────────────────────────────┐
│ LOCAL · Project: lightmem · Connection: codex-dev              │
│ Vault: /projects/lightmem/.lightmem.db                 [Change]│
├──────────────────┬────────────────────────────────────────────┤
│ > Home           │ Ready to remember                          │
│   Memories       │ 248 active · 12 archived · index current   │
│   Review         │                                            │
│   Connections    │ [Find memory] [Add memory] [Ask a question]│
│   Settings       │                                            │
│   Backups        │ Setup                                      │
│   Tools          │ ✓ Local storage    ✓ Search model          │
│                  │ ○ Agent verification not completed         │
│                  │ [Continue setup] [Run diagnostics]         │
│                  │                                            │
│                  │ Recent memories                            │
│                  │ API uses port 8180                         │
│                  │ Prefer Rust for the memory engine          │
├──────────────────┴────────────────────────────────────────────┤
│ ↑↓ Move · Enter Open · ? Help                                 │
└───────────────────────────────────────────────────────────────┘
```

Readiness statements have evidence: saved settings, index identity, model availability, a dated provider check, or a completed explicit integration check. Detecting an application alone does not mark its memory integration as working.

### P02 — Shared guided setup

```text
┌─ Set up LightMem · Step 1 of 5 ─────────────────────────────────┐
│ What would you like to start with?                             │
│                                                               │
│ > Local models                                                │
│   Search and answer on this computer after model downloads.   │
│                                                               │
│   Try without downloading models                              │
│   Basic hash/keyword retrieval and the best matching memory.   │
│                                                               │
│   Use an LLM for answers                                      │
│   Local search with an Ollama or compatible LLM endpoint.      │
│                                                               │
│   Connect to a remote LightMem vault                          │
│   Use a memory server running on another computer.            │
│                                                               │
│ [Back]                                                [Next]  │
└───────────────────────────────────────────────────────────────┘
```

The five steps are: (1) working mode, (2) destination, (3) search/answers and optional extraction, (4) review changes/downloads/data destinations, (5) apply, check, and offer an agent connection. Steps adapt to the selected mode; remote-client setup asks for a server URL/key and supported remote checks instead of requiring local model downloads.

Local models are the default for a new local setup. New guided LLM setup keeps entity extraction off until the user enables it. Existing preset command semantics are retained for compatibility; the wizard applies its explicit extraction choice as an override and shows it in review. Model download sizes are estimates obtained from known assets where available, not hardcoded readiness claims. Downloads begin only after the user chooses the corresponding action.

The shell/PowerShell installer handles binary installation and calls the same setup flow on a terminal. Noninteractive installation uses explicit options and prints the next setup command. Browser onboarding follows the same steps and defaults while using browser forms.

### P03 — Choose vault and context

```text
┌─ Choose memory destination ────────────────────────────────────┐
│ Find [lightmem____________________________________________]   │
│ > Project vault     /projects/lightmem/.lightmem.db            │
│   Personal vault    ~/.lightmem/memories.db                    │
│   codex-dev         alice / coding / lightmem / project        │
│   Remote server     https://memory.example                    │
│                                                               │
│ Selected: codex-dev                                            │
│ Scope [Project v]    Session [current-session______________]   │
│ Actual vault: ~/.lightmem/vaults/<user>/projects/<project>.db   │
│ Selection applies to this workspace session.                  │
│ [Open another local database] [Create connection] [Use vault]  │
└───────────────────────────────────────────────────────────────┘
```

The selector explains why the initial destination was chosen: flag, environment, connection, project discovery, or global fallback. Scope/session controls appear for named connections. Conflicting environment DB/connection selections produce an actionable explanation at launch. A remote target disables local identity attribution and offers remote-supported actions. Setting a persistent remote default is a separate reviewed settings action.

### P04 — Browse, search, and ask

```text
┌─ Memories · LOCAL / lightmem ──────────────────────────────────┐
│ [Search v]  Query [database decision____________________] [Go]│
│ Category [All v] Media [All v] Status [Active v] [More filters]│
├────────────────────────────────┬──────────────────────────────┤
│ > Storage decision             │ Storage decision             │
│   API port                     │ We chose SQLite because...   │
│   Coding preferences           │                              │
│                                │ Decision · #architecture     │
│ 3 matching memories            │ [Open] [Edit] [Relationships] │
├────────────────────────────────┴──────────────────────────────┤
│ [Previous] Page 1 [Next] · [Add memory]                         │
└───────────────────────────────────────────────────────────────┘
```

Modes are Browse, Search, and Ask. More filters expose supported date/as-of, limit, similarity, image-query, precision, and graph expansion options. Only options applicable to a mode/target are offered. Ask shows an answer, engine, and source links. Archived memories remain available in Browse; active-only retrieval behavior is explained next to the mode. An as-of view opens historical results for inspection and requires returning to the current record before editing. Scores are labeled as ranking scores.

### P05 — Memory detail

```text
┌─ Storage decision · <memory-id> ───────────────────────────────┐
│ We chose SQLite because it keeps deployment simple.           │
│                                                               │
│ Category: Decision   Tags: architecture, storage               │
│ Confidence: 0.90     Expires: Never                            │
│ Created: <time>      Updated: <time>                           │
│ Source: <provenance> · codex-dev / project / <session>          │
│ Files: 2            Relationships: 3                          │
│                                                               │
│ [Edit] [Files] [Relationships] [Archive] [Copy ID]              │
│ [Advanced actions]                                            │
└───────────────────────────────────────────────────────────────┘
```

Archived records offer Restore. Permanent deletion is under Advanced actions and states what will be removed. Restoring explains that the previous expiry is cleared. Reading a password-category memory does not imply a new encryption or password-manager feature.

### P06 — Add and edit memory

```text
┌─ Edit memory · LOCAL / lightmem ───────────────────────────────┐
│ Title     [Storage decision_______________________________]   │
│ Category  [Decision v]                                        │
│ Content                                                      │
│ ┌─────────────────────────────────────────────────────────┐   │
│ │ We chose SQLite because it keeps deployment simple.    │   │
│ │                                                       │   │
│ └─────────────────────────────────────────────────────────┘   │
│ Tags      [architecture, storage_________________________]   │
│ > Advanced: confidence · expiry · provenance                 │
│ Files: 2 retained                                  [Manage] │
│ Entity refresh [Clear stale extracted entities v]            │
│                                                              │
│ Unsaved changes                       [Cancel] [Review save] │
└──────────────────────────────────────────────────────────────┘
```

Title, content, category, tags, confidence, provenance, and expiry become editable. Expiry choices are keep current, never, duration from save, or explicit date/time, with the resulting timestamp shown. IDs, creation time, and original source attribution are read-only. Edits update the same record; changing a connection is not part of editing a memory. Lifecycle changes stay explicit Archive/Restore actions.

The normal text editor supports multiple lines. A later enhancement may offer the user's external editor; the initial form does not require one. New-memory mode uses the same form with a destination, optional files, explicit extraction controls, and applicable creation-only options such as supersede in Advanced.

### P07 — Review and save edits

```text
┌─ Review memory changes ────────────────────────────────────────┐
│ Destination: LOCAL / lightmem                                 │
│ Memory: Storage decision · <memory-id>                         │
│                                                               │
│ Content                                                       │
│ - We chose SQLite.                                            │
│ + We chose SQLite because it keeps deployment simple.         │
│ Tags: architecture → architecture, storage                     │
│ Expiry: unchanged                                             │
│ Files: 2 retained                                              │
│ Search representation: rebuild for this record                 │
│ Old extracted entities: remove                                 │
│ LLM refresh: Off                                               │
│                                                               │
│ [Back to editing]                                  [Save edit]│
└───────────────────────────────────────────────────────────────┘
```

Saving must keep the record, full-text index, embedding, deduplication key, and derived data consistent. Compute required embeddings/extraction before committing; if preparation fails, the previous record remains available. Check the captured record/version before the transaction commits; if another client changed it, keep the draft and offer Reload/compare rather than overwrite.

For changes to text/category/tags or relevant attachment metadata, prepare the new search representation. Changes confined to confidence/expiry/provenance do not require embedding when the indexed card text is unchanged. Existing attachments and manually created links are retained unless explicitly changed. Automatically derived relationships/entities must be reconciled so stale claims do not survive an edit. The current schema may need explicit origin tracking for derived links; this is a backend prerequisite, not assumed existing behavior.

The review identifies which extracted entities will be cleared or refreshed. Refreshing with an LLM is a separate opt-in action with the endpoint shown. The initial edit feature does not promise a permanent version-history/undo system. Existing temporal queries do not become an edit-history browser. A later history feature would require a separate data-model design.

### P08 — Attachments

```text
┌─ Files · Storage decision ─────────────────────────────────────┐
│ > design.pdf     Document · 240 KiB                            │
│   diagram.png    Image · 1280 × 720                             │
│                                                               │
│ [Open] [Export to...] [Copy materialized path]                  │
│ [Add files] [Remove from memory]                               │
│                                                               │
│ Pending: add revised-design.pdf; remove design.pdf             │
│ Original file payloads stay immutable.                         │
│ [Back to memory draft]                                        │
└───────────────────────────────────────────────────────────────┘
```

File changes join the memory draft and its final review. Replacement is an explicit remove/add action on stored attachment records, with integrity checks, index refresh, and atomic commit. It is not an in-place rewrite of an immutable blob. LLM interpretation is optional and displays its destination. Remote open/export options are offered only when the client implements the corresponding transport; current unsupported remote flags do not magically become supported.

### P09 — Review conversation candidates

```text
┌─ Review extracted memories · Candidate 2 of 6 ──────────────────┐
│ Source: meeting-notes.txt                                      │
│ Category: Decision    Confidence: 0.91                         │
│ We will use SQLite for the first release.                      │
│                                                               │
│ [Keep] [Edit candidate] [Skip]                                 │
│                                                               │
│ Selection: 4 kept · 1 skipped · 1 pending                       │
│ [Back] [Review selected memories]                              │
└───────────────────────────────────────────────────────────────┘
```

Extraction and saving are separate steps. Candidate edits use the memory form without writing a record yet. Review shows all selected candidates, destination, and tags before saving. Existing direct `observe --dry-run` and `--yes` workflows remain available.

### P10 — Conflicts and duplicates

```text
┌─ Review conflict · Pair 1 of 4 ─────────────────────────────────┐
│ Older                         │ Newer                         │
│ API runs on port 8080.         │ API now runs on port 8180.    │
│ <date/source>                 │ <date/source>                 │
├───────────────────────────────┴───────────────────────────────┤
│ Proposed merge: API runs on port 8180.                         │
│ Result: <surviving record> · <record to retire>                 │
│ [Edit merged text] [Merge this pair] [Skip]                    │
│ [Review remaining pairs]                                      │
└───────────────────────────────────────────────────────────────┘
```

A preview must use the same merge plan the commit applies, and reject stale source records. Generating a model-assisted merge is an explicit operation with the endpoint shown. Similarity indicates overlap; it is not proof of contradiction. Duplicate review is a separate mode with a dry-run plan added to the engine before the UI offers it. Batch resolve is an explicit reviewed action, not triggered by opening this page.

### P11 — Relationships and graph

```text
┌─ Relationships · Storage decision ─────────────────────────────┐
│ > Storage decision                                            │
│   ├─ implements → Deployment plan                             │
│   └─ references → Backup strategy                             │
│                                                               │
│ Radius [2 hops v]   Similarity threshold [0.75]                │
│ [Open memory] [Add link] [Remove link] [Preview auto-links]     │
└───────────────────────────────────────────────────────────────┘
```

Adding a link searches source/target and asks for relation/weight. Removing identifies the exact edge. Auto-linking needs a preview-capable domain operation; its UI displays the proposed edges before applying. Graph expansion stays available in recall as well.

### P12 — Connections and agent setup

```text
┌─ Connect an agent · Step 2 of 4 ────────────────────────────────┐
│ Tool       [Codex v]      Installation [User-wide v]            │
│ User       [alice________] Agent [coding___________________]   │
│ Project    [lightmem_____] Scope [Project v]                   │
│ Connection [codex-dev______________________________________]   │
│                                                               │
│ Memory destination: <resolved vault path>                     │
│ Skill destination: <actual tool configuration path>            │
│ Binding method: [Agent skill v]                               │
│                                                               │
│ [Back]                                     [Review binding]   │
└───────────────────────────────────────────────────────────────┘
```

Steps: choose tool, define/reuse identities and destination, review exact files/binding method, install and verify. Existing IDs are selectable rather than re-created. The generic Agents target and custom harness paths are available in Advanced. MCP is shown only for supported clients. Existing files are read and a concrete change preview is displayed before installation.

Verification separates application detected, binding installed, and save/recall confirmed. The final step provides a test instruction for the user to run in the agent. It does not claim to read or validate a new agent conversation without evidence.

Identity display-name editing can follow the first release of the connection wizard. Changing IDs, ownership, or scope-bound destinations and deleting bindings needs its own migration/unbinding behavior, and is not bundled into a generic Rename button.

### P13 — Settings overview

```text
┌─ Settings · Shared under ~/.lightmem ──────────────────────────┐
│ Find setting [____________________________________________]   │
├──────────────────────┬────────────────────────────────────────┤
│ > Search             │ Search embedding engine                │
│   Answers            │ Value: Local ONNX                       │
│   Extraction         │ Source: Saved configuration             │
│   Storage & routing  │ Default: Local ONNX                      │
│   Credentials        │                                        │
│   Environment & paths│ Converts memory into vectors so queries │
│                      │ can find related meanings.              │
│                      │                                        │
│                      │ Changing this may rebuild embeddings.   │
│                      │ [Edit] [Restore field default]          │
├──────────────────────┴────────────────────────────────────────┤
│ Draft: 0 changes                         [Review changes]     │
└───────────────────────────────────────────────────────────────┘
```

Every setting shows a friendly name, description, stored value, effective value, source, default, valid options, destination/scope, and consequences. Credentials show presence and source, not the secret value. Help can show the corresponding existing CLI flag and raw config key.

Fields relevant only to another backend appear under Advanced with a clear inactive label. They remain searchable so a user can still understand saved values. Search parameters such as result limit, confidence, conflict threshold, and date filters remain operation options; this proposal does not silently add global defaults for every CLI flag.

### P14 — Search, answers, and extraction forms

```text
┌─ Search settings ──────────────────────────────────────────────┐
│ Engine [Local ONNX v]                                         │
│ Model  [BGE small · 384 dimensions v]                          │
│        [MiniLM · 384] [Nomic · 768] [Custom directory...]       │
│ Model status: <cached / missing / unchecked>                   │
│ [Download model]                                              │
│ Changing model can require a vault reindex.                    │
│ [Back] [Review changes]                                       │
└───────────────────────────────────────────────────────────────┘

┌─ Answer settings ──────────────────────────────────────────────┐
│ Answer method [Local extractive QA v]                          │
│ Model [MiniLM SQuAD2 v]                                        │
│ Other methods: TinyRoBERTa / Best matching memory / LLM        │
│ LLM choices show endpoint and answer-model fields.             │
└───────────────────────────────────────────────────────────────┘

┌─ Extraction settings ──────────────────────────────────────────┐
│ Extract entities from newly saved memories [Off v]             │
│ When enabled, memory text goes to <configured LLM endpoint>.   │
│ Attachment interpretation is chosen when adding files.         │
│ Conversation extraction is chosen in Review.                   │
└───────────────────────────────────────────────────────────────┘
```

The forms explain the difference between search embeddings, answer extraction, structured entities, attachment interpretation, and conversation extraction. Enabling one does not implicitly enable the others. Endpoint/model validation follows actual provider capabilities; a generative-only endpoint cannot be selected as if it supplied embeddings.

### P15 — Routing, credentials, and overrides

```text
┌─ Storage & routing ────────────────────────────────────────────┐
│ Current target: Local project vault                           │
│ Saved default target [Local selection v]                       │
│ Remote vault URL [https://memory.example___________________]   │
│ Remote vault credential: <configured / not configured>         │
│ [Test remote connection] [Change remote credential]            │
│                                                               │
│ Model service: http://localhost:11434                          │
│ Model credential: supplied by OLLAMA_API_KEY                   │
│ [Change stored model credential]                              │
│                                                               │
│ Environment overrides:                                        │
│ LIGHTMEM_REMOTE_URL controls remote routing in this process.   │
│ [Explain how to change environment]                           │
└───────────────────────────────────────────────────────────────┘
```

Remote vault credentials and model service credentials have separate labels and forms. Secret entry supports keep/replace/remove stored value. Review redacts values. When an environment credential will remain active after removing a stored credential, the form says so. Inactive/overridden saved values and effective values are both shown.

The initial proposal preserves existing credential precedence, explaining it accurately. Environment variables are read-only in the settings UI; shell/OS configuration is not rewritten. Offline/QA overrides, config root, model cache, binary location, and full selected vault path are inspectable under Environment & paths. Changing configuration root requires a new process, with a copyable launch command. An explicit action changes remote routing after review; the editor does not switch the current working destination as a side effect of typing a URL.

### P16 — Configuration impact review

```text
┌─ Review setting changes ───────────────────────────────────────┐
│ Setting                 Before                After           │
│ Search model            BGE small             Nomic           │
│ Entity extraction       Off                   Off             │
│                                                               │
│ Settings file: ~/.lightmem/config.json                         │
│ Scope: Shared by vaults using this configuration root          │
│ Selected vault: <path> · 248 records need a new index           │
│ Other known vaults: <paths and compatible/stale/unchecked>      │
│ Model download: <assets missing / already cached>              │
│ Network destination: <model download host or model service>    │
│                                                               │
│ [Back] [Download required assets] [Apply and reindex selected] │
└───────────────────────────────────────────────────────────────┘
```

Preview is side-effect free. Other known vaults are inspected read-only where possible, with unknown/unreadable states shown honestly. The impact list covers discovered/registered vaults, not every arbitrary database on disk. The initial flow reindexes only the selected vault after explicit review. Changing the search model does not claim other indexes are ready.

A draft is checked against current configuration and vault state again when applying. A failed migration leaves the prior index/settings available where the existing operation guarantees this; successful work and any later failure are reported separately. The workflow must not claim one atomic transaction spans a SQLite vault, config file, network calls, and other vaults. Recovery states show the actual completed stages and the next valid action.

### P17 — Diagnostics

```text
┌─ Check LightMem ───────────────────────────────────────────────┐
│ Local checks                                                  │
│ ✓ Configuration parses                                        │
│ ✓ Selected vault accessible                                   │
│ ✓ Index matches configured embedding identity                 │
│ ○ Embedding provider not tested                 [Run test]    │
│ ○ Answer engine not tested                     [Run test]    │
│                                                               │
│ Remote vault: not selected                                    │
│ Agent: binding installed; save/recall not verified             │
│                                                               │
│ Checks that need a download/network call explain it first.    │
│ [View details] [Copy diagnostic summary]                       │
└───────────────────────────────────────────────────────────────┘
```

Default `doctor` inspects local state without initializing models or calling remote services. Explicit tests cover embeddings, answer-engine response, remote health/auth/access, and a user-run agent memory cycle separately. Model response does not certify answer quality. Each result has a time and target/config identity so it can become stale after changes. Diagnostic output redacts credentials.

### P18 — Backups and transfers

```text
┌─ Backups & transfers · Destination: LOCAL / lightmem ───────────┐
│ [Create backup] [Import memories] [Restore bundle]             │
│                                                               │
│ Create: Format [ZIP bundle v] Location [Choose destination...] │
│ Other exports: JSON backup / Open Knowledge Format            │
│                                                               │
│ Restore preview                                               │
│ Source: lightmem-backup.zip · <manifest and validation state>  │
│ Destination: <full vault path> · <existing record count>        │
│ Mode [Merge v]                                                │
│ Replacing requires typing the shown confirmation phrase.      │
│ [Back] [Review operation]                                      │
└───────────────────────────────────────────────────────────────┘
```

Import explains merge vs replacement and supported formats. Unsupported enrichment is not offered. The terminal reads local files directly, so the browser's upload limit is not imported into the TUI. Bundle inspection is labeled separately from full validation. Any preparation that would write a backup or materialize a file is an explicit action. Backup timestamps describe a known successful export, not an unverified claim that a file still exists or can be restored.

### P19 — Server controls

```text
┌─ Server on this computer ──────────────────────────────────────┐
│ Status: Stopped                                               │
│ Listen address [127.0.0.1_______] Port [8080]                   │
│ Authentication [Generated token v]                            │
│ Mode [Background v]                                           │
│                                                               │
│ [Review start] [Refresh status]                                │
│ When running: endpoint, health, [Stop server]                  │
│ Changing the address to LAN access shows its reach in review. │
└───────────────────────────────────────────────────────────────┘
```

This manages the local process even when a remote vault is selected elsewhere. The new guided form starts with loopback as its suggested address; the existing direct `serve` default is retained unless a separately documented compatibility change is chosen. No-auth stays an explicit advanced choice. Credentials are not displayed in shared screen summaries or logs; copying a client connection command is a deliberate action. Foreground mode exits the TUI to run the server in the terminal; background mode returns to its status screen.

### P20 — Operations, errors, and unsaved work

```text
┌─ Current operation ────────────────────────────────────────────┐
│ Reindex: LOCAL / lightmem                                     │
│ 148 / 248 memories      [==============...........] 60%        │
│ Current stage: Preparing new vectors                           │
│ Existing index remains available until publication.           │
│ [View details]                                                │
│                                                               │
│ On failure: <specific stage, completed work, recovery action>  │
│ On stale edit: [Compare current record] [Keep draft]           │
│ On exit with edits: [Keep editing] [Discard changes]           │
└───────────────────────────────────────────────────────────────┘
```

Progress is based on real counts when available, otherwise a stage/spinner. The first release shows operations from the current session; persistent task history and cancellation are not promised. A Cancel button is added only for operations that implement safe cancellation. Downloads, model preparation, and vault writes use background work so the interface can keep rendering status.

### P21 — Maintenance

```text
┌─ Maintenance · LOCAL / lightmem ───────────────────────────────┐
│ [Review duplicates] [Reindex selected vault]                   │
│ [Archive all active memories]                                 │
│ > Advanced                                                    │
│   [Permanently delete selected memory]                        │
│   [Clear selected vault]                                      │
│                                                               │
│ Review always names the full destination, count, and effect.  │
└───────────────────────────────────────────────────────────────┘
```

Actions invoke domain operations after a concrete review. Permanent bulk removal uses typed target confirmation. Uninstall remains a direct utility with a clear explanation in Help; removing the running application's binary is not a routine dashboard action. Archive and permanent deletion use distinct wording throughout.

### P22 — Help and command equivalents

```text
┌─ Help · Search settings ───────────────────────────────────────┐
│ What this setting does                                        │
│ Search embeddings help retrieve memories with related meaning.│
│                                                               │
│ Command equivalent                                            │
│ lmem --db '<vault>' config --onnx-model nomic --dry-run         │
│                                                               │
│ Settings are shared; only this selected vault is migrated.     │
│ [Copy command] [Open local documentation] [Back]               │
└───────────────────────────────────────────────────────────────┘
```

The command preview uses the actual target and quotes paths correctly. Secret values are omitted or represented through environment-variable instructions; they do not appear in a clipboard command by default. Help offers shell completion generation, version information, and direct utility commands. Copying a command does not execute it.

## 5. Coverage of every existing CLI family

| Existing command(s) | Proposed terminal home |
| --- | --- |
| `status`, `context`, `stats` | P01 home, P03 destination/context, P17 diagnostics |
| `identity` | P03/P12 identities and connections; advanced registry details |
| `remember`, `inspect`, `list`, `recall`, `answer` | P04–P08 browse/detail/add/edit/files/ask |
| `observe` | P09 candidate review |
| `conflicts`, `deduplicate` | P10 conflict/duplicate review; P21 maintenance |
| `forget`, `restore`, `clear` | P05 lifecycle actions; P21 bulk maintenance |
| `link`, `unlink`, `related`, `graph`, `autolink` | P11 relationship/graph view |
| `import`, `export`, `bundle` | P18 backups and transfers |
| `config`, `setup`, `reindex` | P02 setup; P13–P16 settings/review; P20 operation status |
| `connect` | P12 agent connection wizard |
| `mcp` | P12 binding guidance; P22 stdio launch documentation |
| `serve` | P19 local server controls |
| `ui`, `docs` | P22 launches existing browser workspace/documentation |
| `completions`, `version`, `uninstall` | P22 utility help and direct command actions |

The terminal does not need a separate menu item for every flag or alias. All supported operation parameters are available in the matching form's Advanced controls, or explicitly identified as a direct-CLI action. Unsupported engine behavior remains unavailable until its domain implementation exists.

## 6. Coverage of the nine saved settings

| Stored field | Proposed label/location |
| --- | --- |
| `backend` | Settings → Search → Embedding engine |
| `onnx_model` | Settings → Search → Local embedding model |
| `embedding_model` | Settings → Search → Ollama embedding model |
| `reranker` | Settings → Answers → Answer method/model |
| `entity_extraction` | Settings → Extraction → Extract structured entities |
| `ollama_url` | Settings → Search/Answers service details → Model service URL; one shared field |
| `ollama_api_key` | Settings → Credentials → Model service credential |
| `remote_url` | Settings → Storage & routing → Remote LightMem server |
| `api_key` | Settings → Credentials → Remote LightMem credential |

The field schema is shared rather than copied into three different editors. It defines help, defaults, validation, applicability, source/effective value, and impact. Terminal, browser, and CLI plans use the same validation and preview operations.

Add explicit CLI spellings to resolve the current `--model` ambiguity: `--embedding-model` where configuring search, and `--answer-model` where configuring answers. Existing `config --model` and `setup --model` retain their original meanings as compatibility aliases. Conflicting values fail clearly. CLI help and examples lead with the explicit names.

## 7. Browser and programmatic changes

The terminal is the first delivery priority. The browser then adopts the same memory editor and settings model: memory detail gains Edit; Settings becomes editable using the same Search/Answers/Extraction/Routing/Credentials groups. Models & setup becomes guided setup and model readiness, linking to those shared settings forms. The browser retains its six top-level pages initially; advanced review/graph/server pages can follow the terminal rollout rather than blocking its first version.

New memory-edit behavior needs a controller/repository operation before either interface uses it. HTTP editing, Python edit convenience methods, and any MCP edit tool are separate exposed capabilities that use that operation once available. The initial terminal editor supports local vaults; it advertises remote editing only after the server implements update, conflict/version checks, attachment transport as applicable, and capability discovery. Older servers report unsupported editing with a clear explanation.

Existing stdio/JSON contracts stay compatible. JSON diagnostics and newly introduced edit results are documented and tested as new contracts. MCP tools do not gain broad administrative permissions just because terminal settings exist.

## 8. Implementation structure

Use the existing Rust controller/repository as the operation layer. The terminal workspace runs it in-process and does not require a browser server. Add a terminal view area such as `src/views/tui/` for navigation, forms, dialogs, and rendering. UI event handling calls structured operations, rather than parsing the existing colored CLI output.

Extract shared configuration descriptions, validation, and preview/apply planning out of the CLI/workspace handlers. Share plans for edit, merge, duplicate review, auto-link preview, transfer, and binding installation so the preview the user sees matches the action applied. Use workers for model/network work and revision checks for writes.

The first editor requires atomic record/index updates, precise stale-write rejection, explicit entity/derived-link handling, and attachment-preserving semantics. Attachment replacement adds immutable-record delete/add operations under the same edit transaction. Existing insert/import/merge routines should be reused only where their behavior matches editing; an import overwrite is not automatically an edit API.

Configuration, identity, remote dispatch, and sensitive operations remain behind explicit operation capabilities. The first launch derives available actions from the actual local/remote target; a later server capability response makes remote feature availability reliable across versions.

## 9. Delivery order and acceptance

| Stage | Concrete delivery | Acceptance examples |
| --- | --- | --- |
| 1. Understand and configure | P01–P03, P13–P17, P20/P22 basics; shared field/preview model; explicit CLI aliases | New user reaches a working demo/local setup; every saved setting has help/source/impact; preview is side-effect free; override explanations match effective behavior |
| 2. Browse and edit | P04–P08 and supporting controller/repository update; browser edit reuse after local terminal validation | Edit a record without changing ID/source context or losing files/manual links; search reflects content changes; failed/stale saves preserve prior record and draft; historical views are read-only |
| 3. Connect and protect data | P12, P18, P19, P21; consistent installer handoff | Review binding files before install; distinguish installed from verified; inspect/merge/replace correct target; start/stop server and report actual status |
| 4. Review and relationships | P09–P11; merge/dedup/autolink plan operations | Edit extracted candidates; preview exact merge/edge changes; reject stale plans; no batch resolution just from opening a page |
| 5. Extend interface parity | Editable browser settings, selected advanced browser views, documented HTTP/Python/MCP edit capabilities | All clients call the same validated operations; old clients/servers give predictable results; JSON and direct CLI checks still pass |

Stage 1 makes configuration easier to understand; stage 2 delivers the requested editing capability. Together they are the first useful release of the new experience. Further stages expose the rest of the engine through reviewed workflows.

Validation should target real risks: terminal focus/text entry and clean exit, narrow/no-color rendering, dirty-form navigation, correct target resolution, effective credential precedence, no side effects during preview, atomic edit/search behavior, stale saves, preserved files/manual links, derived-data refresh, migration/download failures, remote unsupported operations, and existing CLI/JSON/Python contract regressions. Use representative workflows and backend invariants rather than a test that merely asserts each label exists.

## 10. Review sequence

Start with P01 Home, P02 Setup, P06/P07 Editing, and P13/P16 Settings/review. Those define the everyday experience. P03 target selection and P15 routing/credentials establish where actions go. The remaining screens can then be reviewed against that common navigation, form, and review pattern.

This change adds one proposal document. No application implementation, schema migration, local settings, vault records, or external agent configuration was changed while preparing it.
