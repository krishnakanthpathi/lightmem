# Everyday terminal commands

LightMem remains a command-line tool. It adds short setup/configuration prompts,
completion hints, and diagnostic explanations. There is no full-screen interface
or new memory-edit command.

## Setup

Run `lmem setup` in a terminal and press Enter for local ONNX search, MiniLM
extractive answers, and entity extraction off. Decline the recommended choice for
a no-download demo, LLM answers, or a remote LightMem vault. The configuration
preview identifies the settings file, selected vault, and migration requirement.
Nothing is saved until the final confirmation. EOF and cancellation leave settings
unchanged. Model preparation is offered separately; offline mode skips downloads.

A local preset configures models; saved/environment remote routing is separate.
The guided flow offers to clear saved routing. Environment routing must be changed
in the shell, or bypassed with an explicit local `--db`/`--global` selection.

Scripts specify their choices:

```sh
lmem setup --preset demo --dry-run --json
lmem setup --preset demo --yes --json
lmem --db ./project.db config --preset local --dry-run --json
```

`demo` uses deterministic hash retrieval and top-memory answers; it is not neural
semantic search. Local ONNX models need cached assets or a first download.
The installers now hand off to this same setup rather than presenting different
model/LLM menus. Noninteractive installation leaves configuration alone unless
installer environment options explicitly request settings/models. Skip-model flags
continue to skip model downloads.

## Configuration

`lmem config --edit` prints a numbered list: Search, Answers, Extraction, Remote
LightMem connection, or Credentials. Choose a group, enter its values, review the
changed fields and selected-vault impact, and confirm. Blank answers keep current
field values. Credential replacement uses hidden input. Removing a saved key may
expose an environment fallback, which the prompts explain. Concurrent external
configuration changes abort the save rather than overwrite them.

Direct flags and new clearer aliases:

```sh
lmem config --onnx-model minilm --dry-run
lmem config --embedding-model nomic-embed-text
lmem config --answer-model top1
lmem setup --answer-model minilm-squad2 --dry-run
lmem config --entity-extraction off
```

`config --embedding-model` retains the meaning of `config --model` (Ollama search
model). `config --answer-model` retains `config --reranker`, and
`setup --answer-model` retains `setup --model`. Supplying conflicting aliases is
an error. Custom ONNX directories and custom Ollama names still work through the
original flags; completion suggestions do not restrict runtime model choices.

Configuration is shared across vaults within `LIGHTMEM_CONFIG_DIR` (by default
`~/.lightmem`). Embedding migration applies to the selected vault and requires
explicit approval. Preview does not save settings, create a vault, or download
models. An embedding change rebuilds the index before publishing the new settings.

`lmem config` shows effective routing and credential sources. Model credentials
prefer a nonempty stored key, then `OLLAMA_API_KEY`, then `LMEM_OLLAMA_API_KEY`.
Remote credentials prefer `LIGHTMEM_API_KEY`, then `LIGHTMEM_KEY`, then the saved
key. Remote URLs prefer `LIGHTMEM_REMOTE_URL`, then `LIGHTMEM_URL`, then saved
routing. Environment values are not rewritten by config prompts. JSON inspection
and previews redact both credential fields.

## Readiness

`lmem doctor [--json]` reads configuration and inspects the selected vault/index
without initializing a controller, creating a vault/registry, loading models, or
contacting a remote server. Malformed settings and incompatible indexes produce a
nonzero exit status and a structured result with `--json`. Missing vaults are
reported as not created. Existing cache directories are not presented as verified
model assets.

Use `lmem config --test` for an explicit embedding-provider test; this can initialize
or download models. Test answers separately with a known memory, and remote access
with `lmem context`. Detecting an agent application or installing a binding does not
verify a successful save/recall cycle in that agent.

## Completion

For this session:

```sh
source <(lmem completions bash)
source <(lmem completions zsh)  # Zsh completion system must already be initialized
lmem completions fish | source
```

The generated scripts cover commands, flags, model/preset choices, and scopes.
Bash, Zsh, and Fish also request registered connection IDs at completion time, so
new connections do not require regenerating the script. The lookup validates IDs,
uses a read-only registry connection, and never opens a vault or loads a model.
PowerShell and Elvish retain generated static completion.

`config --edit` is optional and terminal-only. Piped input, JSON commands, and
explicit flags retain normal command behavior. The existing observe/conflicts
review prompts remain line-oriented.
