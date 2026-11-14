# LightMem project and UI audit

Reviewed 14 November 2026, source commit `f394180`, package version `0.7.9`.

Follow-up: the 0.8.0 implementation and verification instructions are in [the review guide](review-0.8.0.md). This audit records the original findings before those changes.

The engine has substantial implemented functionality and a passing existing test suite. The highest-priority problems are inconsistent local/remote behavior, unsafe API defaults, and unclear configuration boundaries. The website already has a coherent visual style; the missing product surface is a workspace for viewing memories, selecting the right vault, configuring models, and checking integrations.

This is an audit and implementation backlog. Application code has not been changed. Runtime reproductions used synthetic records, temporary configuration directories, and loopback servers. No personal memory vault was used for the targeted checks.

## What was checked

- Rust CLI/controller/storage, remote HTTP API/client, model configuration and extraction paths, MCP routing, Python SDK, packaging and CI definitions, documentation, and static UI sources.
- Landing page and documentation in the browser at desktop and phone sizes. Documentation search, keyboard activation of results, command disclosure, installation tabs, internal anchors, and console warnings/errors were checked.
- `LIGHTMEM_QA_DISABLE=1 cargo test --locked --lib --bins --tests`: **227 passed**.
- Python SDK unittest discovery against the compiled binary, using an isolated hash-embedding configuration: **27 passed**.
- Script-safety and benchmark-reporting Python tests: **9 passed**.
- `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`: **passed**.

Passing tests do not cover the new reproductions below. Live ONNX answer quality, live multimodal model compatibility, cloud services, release artifacts on other operating systems, and load behavior were not validated. This review does not establish that every defect has been found.

## Confirmed defects and implementation gaps

Priority P1 means fix before demonstrating or relying on the affected remote/data-management flow. P2 means important correctness or usability work. “Reproduced” means an additional runtime check confirmed the behavior; “source verified” means the current implementation establishes the gap, but a dedicated runtime reproduction was not run.

### B01 · P1 · Invalid clear requests permanently delete records — reproduced

**Evidence:** An authenticated `POST /v1/clear` with body `{invalid` returned HTTP 200 and `{"cleared":3}`. The disposable vault then contained zero records. Deserialization errors become `ClearPayload::default()`, whose `soft` field is false; this invokes hard deletion. An empty body also selects that default.

**Source:** [api_server.rs:752](../src/services/api_server.rs#L752).

**Fix:** Return 400 on malformed JSON or an invalid payload shape. Require explicit intent for hard deletion; preserve data on validation failure. Prefer a recoverable archive operation in a future UI, with a separate permanent-delete action.

**Acceptance:** Malformed JSON, wrong field types, and omitted destructive intent leave record and attachment counts unchanged.

### B02 · P1 · A blank API key silently disables authentication — reproduced

**Evidence:** Starting a temporary server with `--api-key '   '` allowed `/v1/stats` without any credentials, returning HTTP 200. This happens without `--no-auth`. The default listen interface is `0.0.0.0`, increasing the consequence of this input mistake.

**Source:** [api_server.rs:784](../src/services/api_server.rs#L784), [main.rs:687](../src/main.rs#L687).

**Fix:** Reject an explicit empty/whitespace key, or generate a key. Disable authentication only through the explicit `--no-auth` path. Apply the rule to foreground and daemon launches.

**Acceptance:** Missing keys are generated; blank keys fail safely; unauthenticated protected requests return 401 unless no-auth was explicitly selected.

### B03 · P1 · Configuration JSON exposes the remote secret — reproduced

**Evidence:** With synthetic secrets, `lmem config --json` returned `api_key: "synthetic-remote-secret"`, while `ollama_api_key` was correctly `[REDACTED]`. A user displaying configuration during a presentation could reveal a working remote credential.

**Source:** [cli_view.rs:1198](../src/views/cli_view.rs#L1198).

**Fix:** Redact both secret fields by default and expose booleans indicating configuration status. A future settings screen should use masked fields and avoid returning stored credentials to its frontend.

**Acceptance:** Neither synthetic secret appears in default configuration output or diagnostic exports.

### B04 · P1 · Unsupported remote commands operate locally — reproduced

**Evidence:** `lmem --remote http://127.0.0.1:9 export --json -o ...` succeeded and wrote a local backup even though that remote endpoint was unreachable. The remote handler returns false for unsupported commands, and execution continues through local command dispatch. This also affects unsupported operations such as import, conflicts, and reindex; a user can act on a different vault from the one they intended.

**Source:** [main.rs:1217](../src/main.rs#L1217), [main.rs:1396](../src/main.rs#L1396).

**Fix:** Define an explicit capability map. In remote mode, reject unsupported vault operations before opening local storage. Deliberately local commands such as help, version, docs, and client configuration can remain available with clear semantics.

**Acceptance:** An unsupported remote vault operation fails without creating, reading, exporting, or modifying a local vault.

### B05 · P1 · Remote mode bypasses named connection scope and attribution — reproduced

**Evidence:** A `remember` call with a registered `--connection auditconn` and `--remote` stored the memory in the server's vault, returned no connection context, and left the named local vault empty. Remote requests contain no selected user/project/agent vault binding. Saved remote configuration can also take precedence over a named connection.

**Source:** [main.rs:1386](../src/main.rs#L1386), [main.rs:930](../src/main.rs#L930), [api_server.rs:25](../src/services/api_server.rs#L25).

**Fix:** Initially reject combinations of remote mode and local connection/scope selectors. If remote scoped vaults are implemented later, define server-side vault resolution and source attribution explicitly. Local identities are organizational identifiers, not authentication.

**Acceptance:** A selected connection can never silently resolve to an unrelated server vault. Unsupported combinations explain how to select a supported target.

### B06 · P1 · Background launch changes the selected scope — reproduced

**Evidence:** `--connection auditconn --scope agent context --json` selected an agent vault, but launching `serve -d` with the same selectors served the connection's default `user.db`. The daemon child receives the connection ID but does not receive the scope or session overrides.

**Source:** [main.rs:3095](../src/main.rs#L3095).

**Fix:** Forward every relevant selector or pass a validated, resolved launch specification. Verify daemon readiness against the expected vault instead of checking only whether the process exists.

**Acceptance:** Foreground and background launches with identical selectors report the same database path and preserve applicable attribution.

### B07 · P1 · Local answer settings still send memory text to an LLM endpoint — reproduced

**Evidence:** Using hash embeddings to avoid model downloads, with the answer reranker set to local `minilm-squad2`, a normal `remember` sent the synthetic memory text to the configured endpoint's `/v1/chat/completions`. The entity extractor uses Ollama independently of whether the answer engine is local. The `local` preset selects local embeddings/QA but does not disable entity extraction or clear an earlier remote Ollama URL. Consequently “100% offline” is not a reliable description of that preset's complete write path.

**Source:** [memory_controller.rs:571](../src/controllers/memory_controller.rs#L571), [observer.rs:1290](../src/services/observer.rs#L1290), [config.rs:135](../src/models/config.rs#L135).

**Fix:** Give entity extraction its own explicit mode: off, local heuristic, or configured LLM. Make the offline preset enforce a consistent network policy across extraction, embeddings, and answers. Show the destination when enabling LLM extraction. `--no-entities` currently avoids this extraction path.

**Acceptance:** A cached offline setup makes no LLM requests during remember/observe/recall/answer. An enabled network extraction mode clearly identifies its destination.

### B08 · P2 · Remote title lookup corrupts Unicode and URL characters — reproduced

**Evidence:** A stored title `Audit café` could not be inspected remotely by title. With both `Audit A` and `Audit A&B` present, inspecting `Audit A&B` returned the **wrong record**, `Audit A`. The client concatenates terms into a URL; the server decodes UTF-8 bytes individually into characters.

**Source:** [remote_client.rs:197](../src/services/remote_client.rs#L197), [api_server.rs:181](../src/services/api_server.rs#L181).

**Fix:** Use structured URL query encoding on the client and proper UTF-8 percent decoding on the server. Apply it to get, list, related, and graph parameters.

**Acceptance:** Exact lookups round-trip Unicode, spaces, `&`, `+`, `#`, and `%`, including when a truncated title is also a valid record.

### B09 · P2 · A positive timezone offset silently removes the remote date cutoff — reproduced

**Evidence:** Remote CLI `list --as-of 2000-01-01T00:00:00+05:30` returned the current test records. The correctly percent-encoded HTTP equivalent returned zero. A literal `+` becomes a space, parsing fails, and the API treats the cutoff as absent. Other invalid list query values also fall back silently rather than returning validation errors.

**Source:** [remote_client.rs:183](../src/services/remote_client.rs#L183), [api_server.rs:602](../src/services/api_server.rs#L602).

**Fix:** Encode parameters and reject invalid supplied filters. Use checked pagination arithmetic as well; the API's page multiplication should follow the overflow validation used in other interfaces.

**Acceptance:** Positive/negative offsets and UTC produce equivalent cutoffs; malformed filters return 400 instead of broadening results.

### B10 · P2 · Remote list ignores the media filter — reproduced

**Evidence:** `/v1/list?media=image` returned two text-only memories. The client sends `media`, but the API list route never applies it.

**Source:** [api_server.rs:561](../src/services/api_server.rs#L561).

**Fix:** Apply media filtering before calculating count, page boundaries, and `has_more`, using the same semantics as local listing.

**Acceptance:** Image-only pages contain only matching memories and report correct filtered totals.

### B11 · P2 · Remote remember silently drops requested behavior — partly reproduced, source verified

**Evidence:** `remember --confidence 0.42` returned a stored confidence of `0.9` remotely. The handler also ignores `--supersede`, `--extract`, and the extraction reranker. The request type cannot carry those options. TTL is converted to whole seconds, losing supported subsecond precision. Entity warnings returned by the controller are discarded by the HTTP route.

**Source:** [main.rs:898](../src/main.rs#L898), [api_server.rs:25](../src/services/api_server.rs#L25), [api_server.rs:459](../src/services/api_server.rs#L459).

**Fix:** Preserve supported options in a shared contract and implement their server behavior. Reject unsupported options with a clear message. Return warnings in a structured response instead of losing them.

**Acceptance:** Local and remote storage preserve the same confidence, supported TTL, and supersession behavior. Unsupported extraction modes fail before writing.

### B12 · P2 · Remote attachment and inspection options are accepted without their effects — source verified

**Evidence:** Remote dispatch ignores recall/inspect/list path or save destinations, media opening, and visual query input. Remote inspect renders an empty links list rather than retrieving graph links. There is no attachment-download route in the current API to implement local path materialization.

**Source:** [main.rs:954](../src/main.rs#L954), [main.rs:1018](../src/main.rs#L1018), [main.rs:1050](../src/main.rs#L1050).

**Fix:** Add authenticated attachment retrieval and graph-aware inspection, or reject these options in remote mode. Avoid successful output that implies files were saved or opened.

**Acceptance:** Every accepted option has its documented effect; otherwise the command fails with an actionable capability error.

### B13 · P2 · Precision recall changes meaning across modes — source verified

**Evidence:** Local `recall --precision` forces ONNX QA and returns an answer; it does not use the configured answer reranker. The remote request has a `precision` field, but the recall route never reads it and returns ordinary recall results. This can change both semantics and JSON shape when changing backend mode.

**Source:** [main.rs:1521](../src/main.rs#L1521), [api_server.rs:482](../src/services/api_server.rs#L482).

**Fix:** Decide whether recall always returns ranked memories and answer always returns an answer. If precision recall stays, give it one explicit cross-interface contract and honor or reject it consistently.

**Acceptance:** Equivalent local/remote requests use the same engine-selection rules and response schema.

### B14 · P2 · Import enrichment is a no-op — source verified

**Evidence:** `import --enrich` reaches `parse_json_candidate(..., _enrich)` without changing behavior. The flag promises functionality that is not implemented.

**Source:** [importer.rs:273](../src/services/importer.rs#L273).

**Fix:** Implement a defined enrichment stage with preview, or remove/reject the option until it works.

## Product and documentation ambiguities

| Problem | Evidence / consequence | Recommended correction |
| --- | --- | --- |
| Landing release badge is stale | [index.html](../static/index.html) says 0.7.7; Cargo/Python/docs say 0.7.9. | Generate release text from one version source; link it to release notes. |
| The complete web CLI reference omits `serve` | Browser search for `serve` returned `observe` first. There is no server command entry or remote setup guide. | Add server lifecycle, auth, client connection, and supported-command documentation; rank exact command tokens ahead of substring matches. |
| Platform documentation is stale | The release workflow now includes Linux arm64 and Windows arm64, while the docs describe Linux arm64 as missing. Windows has no installer tab. | Derive an accurate support matrix from release targets and add PowerShell instructions. macOS x86_64 remains absent from the current release build matrix. |
| First-memory tutorial requires a missing file | The main copyable walkthrough immediately uses `./arch.png`; the Python example similarly requires a local image. | Make the first success text-only and independently executable. Put attachments in a separate optional tutorial with a provided sample. |
| Saving a memory versus recording a conversation | Integrations can sound automatic; the README correctly says agents must invoke remember/observe. | Explain explicit capture on the landing page and show the last successful write per connection. |
| Cloud/offline/preset terms mix independent layers | Storage location, embeddings, entity extraction, and answering have different network behavior. | Show each layer separately and provide plain-language presets with an effective configuration preview. |
| “Conflict” includes similarity and duplicates | Similarity does not establish a factual contradiction; default merge can choose one record instead of preserving every clause. | Use “Potential overlaps” or explain the detector; show old/new/merged text and a diff before applying merges. |
| Strong performance claims lack one reproducible baseline | README claims sub-millisecond retrieval and compares broad competitor classes; actual operations vary by model, cold start, corpus, and network. | Publish measured p50/p95 with hardware, corpus size, model, warm/cold state, and command. Distinguish CLI startup from retrieval and QA. |
| “Zero idle RAM” depends on process lifetime | An exited CLI consumes no process memory; running MCP/HTTP processes remain alive and can retain loaded models. | State the runtime mode and measured idle behavior explicitly. |
| A vault/password category can imply encryption | Current storage uses SQLite and file permissions; a password category is not encryption at rest. | Clearly describe storage protection and avoid presenting the category as a password-manager feature. |
| Some caveats describe already-fixed or obsolete behavior | Docs still reference obsolete graph flags in SKILL.md; the current skill uses terminal/JSON syntax. The skill still has invalid positional export examples at lines 126–127. | Validate command examples against Clap; remove obsolete historical notes from the primary guide. |
| Internal implementation detail overwhelms the user path | The main docs discuss unused dependencies, mutex poisoning, and old integration history before users have a configured workspace. | Keep a short getting-started guide; move internals to architecture and maintainer notes, retaining material user-facing limits near affected features. |

## Release engineering follow-up

- **Linux wheel compatibility is not established by retagging.** The workflow builds on `ubuntu-latest`, then rewrites the wheel tag to `manylinux2014_x86_64` without a manylinux build/repair check. This is a source-verified compatibility risk, not a reproduced Linux failure. Build on the target baseline, audit native dependencies, and test the installed wheel on that baseline. See [pypi.yml](../.github/workflows/pypi.yml) and [build_wheels.py](../python/build_wheels.py).
- **Validation runs through release workflows, not ordinary PRs.** `validate.yml` is callable only; the current callers run for tags/releases/manual dispatch. Add push/pull-request validation so failures are caught before release. Include UI checks and strict Clippy in the appropriate gate.
- **The custom HTTP server needs bounded resource use before being described as high throughput.** It spawns a thread per connection, reads headers with no fixed total limit, and parses/allocates request bodies before authentication. Use bounded concurrency, explicit parsing limits, and appropriate auth/CORS policy. Load behavior was not tested in this audit.
- **Downloaded executable/model integrity deserves an explicit policy.** The Python fallback extracts release archives directly; QA assets lack a pinned content digest. Add safe archive-member validation and published digest/signature verification. No malicious artifact was used or vulnerability exploit attempted.

## What the existing UI does well

- Consistent dark palette, typography, spacing, code presentation, and a recognizable accent.
- Static rendering without a client application build or external font dependency.
- Native disclosures, skip links, focus styles, search announcements, keyboard navigation, and reduced-motion support are already present.
- At the checked desktop and phone sizes, documentation did not widen the page; internal hash links resolved and no console errors/warnings were observed.

The first UI investment should make configuration and state visible. Additional animation or decorative cards will have less impact on usability.

## Proposed product workspace

Retain the public landing page and developer docs. Add a separate local management workspace as a new product capability. The current HTML is not a memory-management app, and the current HTTP API is not sufficient to implement all the screens below.

```text
LightMem   Target: This Mac   Connection: codex / myapp   Scope: Project
           Vault: myapp       Models: Ready               + Add memory

Overview   | Search or ask about this vault…
Memories   | All memories · Facts · Decisions · Files · Archived
Connections| -------------------------------------------------
Models     | Searchable list             Selected memory detail
Backups    | Title, preview, tags         Full content, source
Settings   | Updated time, status        Attachments, relationships
           |                             Archive / inspect actions
```

### 1. Guided setup

Flow: choose local/remote target → choose a supported setup → prepare required models → select/create a vault connection → connect an agent → save and retrieve a test memory.

Explain embeddings as “how memories are found” and the answer engine as “how answers are produced.” Show download sizes and progress, connection failures with retry actions, required dependencies, and a final configuration preview. Keep custom URLs and raw model IDs behind Advanced options. A remote target must not offer scoped connections until the server supports them.

### 2. Overview

Show the active target and vault, active/archived memories, index readiness, missing model assets, recent writes, connected tools, and last backup. Every health issue should have a specific next action: download a model, test an endpoint, reindex this vault, or connect a tool. Avoid showing a green “ready” indicator based solely on the unauthenticated health endpoint.

### 3. Memories

Use one searchable list and a detail panel. Provide category/tag/media/date/status filters and a clear distinction between search results and generated/extracted answers. An answer should show its source memory, answer engine, and meaningful no-evidence state. Retrieval scores should be labeled as ranking signals rather than probabilities.

Add a straightforward memory form with title, text, category, tags, and attachments; hide confidence, TTL, and provenance under Advanced. Provide image/file previews, actual save/download actions, and explicit loading/error/empty states. An empty vault should offer “Add your first memory” and disposable demo data. Editing, restoring, and history need backend support before they can be promised.

Keep graph exploration as a secondary view within Memories. Provide a selected-node detail panel and a text/list alternative; a graph alone is a poor first screen.

### 4. Connections

Use per-tool setup cards showing skill versus MCP support, the actual selected vault/scope, installation status, configuration location, and last successful operation. Offer “Test connection” and copyable configuration. Acknowledge when an editor must reload. A green installed badge should not imply that conversations are being automatically captured.

### 5. Models and configuration

Show storage destination, embedding provider/model, answer engine/model, and entity extraction as separate settings. Display the effective value and whether it came from defaults, configuration, environment, or an explicit override.

Configuration is currently shared under the configuration root while vector indexes belong to vaults. The UI must show which changes are global, which vault is being reindexed, which other vaults need migration, and what happens if migration fails. Preview and validate changes before applying them; provide progress and preserve the working configuration on failure.

### 6. Backups and recovery

Offer bundle export, archive inspection, and restore preview. Explain merge versus replace with destination path and record counts before a replacement. Show the last backup and verification result. Use recoverable archive as the normal removal action; separate permanent deletion and conflict-merge review from routine browsing. A true undo/restore experience needs an implemented backend contract.

### 7. Visual polish and usability

- Keep the existing brand and accent; use the accent mainly for primary actions and selected states. Use distinct success, warning, and error treatments with text labels.
- Increase documentation reading sizes and avoid relying on tiny muted labels for important configuration state. Check button contrast and phone touch targets as part of a dedicated accessibility pass.
- Use a persistent context bar so the user always knows where a write will go. Do not show independent selectors that permit invalid target/connection/scope combinations.
- Keep advanced controls collapsed until needed. Explain fields with concrete examples and show validation next to the field.
- Include skeleton/loading states, retry actions, zero results, unavailable server, unavailable model, stale index, and partial import states from the start.
- Add a concise “Try the demo” journey using synthetic data. Show a decision saved in one session and recalled from another; distinguish an illustrative UI from a working feature.

## Implementation order

1. **Correctness before the remote presentation:** B01–B07, then URL/filter/option parity. Add targeted regression tests; publish an honest local/remote capability matrix.
2. **Improve the current website immediately:** align version and platform content, add `serve` and Windows installation, make quick start text-only, correct skill examples, explain explicit capture and offline behavior, simplify the main documentation path.
3. **Build a focused workspace MVP:** local target only initially; onboarding, visible vault context, memory list/search/details, add/archive memory, model readiness, and connection diagnostics. Add the API contracts needed for configuration and model jobs instead of shelling out from a public page.
4. **Add advanced workflows:** authenticated remote management after parity is established, attachment downloads, backup/restore preview, conflict diffs, recovery, and graph exploration.

For the presentation, use the verified local store → recall/answer → shared-agent workflow with pre-cached models and synthetic data. Describe the management workspace as planned until implemented. Avoid demonstrating destructive restore/merge or claiming remote parity while the above gaps remain.
