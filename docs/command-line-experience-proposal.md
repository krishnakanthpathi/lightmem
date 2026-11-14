# LightMem: simpler everyday terminal commands

Revised proposal · 14 November 2026 · Not implemented.

The user prefers ordinary terminal commands. The earlier full-screen terminal workspace proposal is superseded. The direction is a command-first CLI with sensible defaults, Tab completion, useful output, and optional short prompts that stay in terminal scrollback. No persistent dashboard or screen navigation is planned.

Memory editing is excluded from this proposal following review. Configuration prompts remain in scope.

## 1. Everyday use

Most work should require a short command:

```sh
lmem setup
lmem remember "The API uses port 8180"
lmem recall "API port"
lmem answer "Which port does the API use?"
lmem list
```

These commands already exist. The improvement is consistent setup, discoverable defaults, completion, and clear next actions, rather than introducing a new command language.

## 2. Defaults that remove unnecessary questions

| Choice | Proposed everyday behavior |
| --- | --- |
| Search | Local ONNX with BGE small |
| Answers | Local MiniLM extractive QA |
| Entity extraction | Off until explicitly enabled |
| Destination | Existing explicit/environment/connection/project/global selection rules; show the resolved target clearly |
| Category/title | Keep existing automatic inference when omitted |
| Confidence/result limits | Keep existing command defaults and explain them in help |
| Advanced options | Available through flags; normal use does not require them |
| Model download | Explain missing assets and offer a specific download action |
| Remote routing | Explicit reviewed configuration, with saved/environment routing identified |

Most local defaults and automatic memory fields already exist. The work is to make setup, help, and output agree about them. A demo remains available for users who want to try without model downloads. An existing saved configuration is inspected and preserved unless a requested change is applied.

## 3. Short setup with Enter-to-accept defaults

```text
$ lmem setup

Use recommended local settings? [Y/n]:

  Search:       BGE small on this computer
  Answers:      Local MiniLM
  Extraction:   Off
  Vault:        <resolved database>
  Settings:     Shared under <configuration root>
  Models:       <cached / download needed>

[If assets are missing, offer the required downloads explicitly.]
[If embeddings would change, show migration details explicitly.]

Apply these settings? [y/N]:
```

Choose `n` at the first question to see a short numbered list: local models, no-download demo, LLM answers, or remote LightMem. Follow-up questions appear only for that choice. Current-vault migration and other-vault impact remain visible. The default path does not ask new users to choose among every supported model.

The installer hands off to the same setup behavior so installing and running setup explain the same defaults. Existing explicit/noninteractive setup flags remain supported.

## 4. Edit settings through flags or short prompts

Existing flag-based edits continue:

```sh
lmem config --onnx-model minilm
lmem config --entity-extraction off
```

Proposed optional guided edit:

```text
$ lmem config --edit

What would you like to change?
  1. Search
  2. Answers
  3. Extraction
  4. Remote LightMem connection
  5. Credentials

Choose: 2
Answer method [minilm-squad2]: top1

  Answers: minilm-squad2 → top1
  Settings shared under <configuration root>.
  No embedding migration required.

Save? [y/N]:
```

This is a sequence of normal printed prompts. Number entry always works; it does not depend on a full-screen menu or cursor-controlled form.

`lmem config` shows a concise summary with the effective value and its source where relevant. Environment overrides are explained and remain read-only. Model service credentials and remote-vault credentials are labeled separately, with secrets redacted.

Add explicit `--embedding-model` and `--answer-model` spellings where relevant. Existing `config --model` and `setup --model` preserve their current meanings for compatibility. All nine saved fields remain accessible; environment/path information is inspectable without turning every runtime option into a saved preference.

## 5. Make Tab useful

Improve the existing shell-completion support and make installation discoverable during setup. Suggest commands, flags, enum values, model aliases, connection IDs, and file paths where applicable. Shell support determines the available completion behavior.

```text
lmem rec<Tab>                    → recall
lmem config --backend <Tab>      → onnx / ollama / hash
lmem --connection <Tab>          → known connection IDs
```

Command/flag completion already exists. Better value/context completion and clearer activation instructions are proposed improvements. Completions do not load models, download assets, or write vault data. Any optional inline picker should remain confined to an explicit short prompt; no TUI dependency is required for the basic flow.

## 6. Output and diagnostics that explain the next step

```text
$ lmem
LightMem · <active count> active memories
Vault: <resolved path>
Search: BGE small · Answers: MiniLM · Extraction: Off

Start: lmem remember "a fact"
Find:  lmem recall "a question"
Setup: lmem setup
```

Proposed `lmem doctor` inspects config, target, cache, and index compatibility without loading a model or contacting a remote service by default. Explicit provider/remote tests explain their network/download behavior. Agent detection, installed binding, and verified save/recall remain distinct results.

Errors identify the cause and one relevant next command. Provider readiness, answer quality, and remote server access are not collapsed into a generic “everything works” result. Human output stays concise; JSON output retains its documented contracts.

## 7. Remaining workflows

Keep the existing command families and improve their help, defaults, target information, and receipts:

- `observe`: retain short accept/skip prompts and explain what will be saved.
- `conflicts`: show original records and proposed merge before an interactive commit.
- `connect`: show selected tool, binding destination, and verification instructions.
- `import`/`export`/`bundle`: clearly state source, target, format, merge/replacement behavior, and result.
- `graph`/`related`/`link`/`unlink`/`autolink`: retain ordinary commands and readable terminal output.
- `forget`/`restore`/`clear`: distinguish archive, restore, and permanent deletion, with the actual target named.
- `serve`: retain start/status/stop commands with concise status output.
- `identity`/`status`/`context`/`stats`: explain resolved destination and identity consistently.
- `docs`/`ui`/`mcp`/`completions`/`version`/`uninstall`: keep their existing roles and direct entry points.

Prompt when input is missing only in an explicitly interactive mode on a terminal. Piped input and `--json` never unexpectedly become interactive. Supplied flags should be sufficient for an operation, subject to existing explicit approval requirements for migration/destructive actions. Prompt defaults do not authorize unrelated bulk or destructive actions.

## 8. Delivery order

1. Align defaults, concise help/output, completion discovery, and short setup.
2. Add `config --edit` with shared validation/preview and explicit model flag names.
3. Add diagnostic explanations and improve existing review/integration/transfer prompts.

Acceptance focuses on command simplicity and actual correctness: short happy-path commands, flags that fully specify scripted operations, correct target/source explanations, unchanged existing JSON behavior, side-effect-free completion/preview, and configuration validation and migration behavior.

This revision changes proposal documents only. No TUI, CLI feature, application configuration, or vault change has been implemented.
