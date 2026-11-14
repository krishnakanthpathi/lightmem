# CLI review for LightMem 0.8.0

Review the terminal product with this guide. The existing HTML/browser work is retained as reference and is optional. Nothing has been released or published. Use the binary from this checkout, not an older installed `lmem`.

## What changed in the CLI

| Area | Behavior to review |
| --- | --- |
| Guided setup | `lmem setup` offers terminal choices, displays the proposed shared settings and selected vault, and asks before saving. Default is local ONNX. Demo mode needs no downloads; Custom LLM accepts an endpoint and model in the terminal. Invalid choices fail without changing settings. |
| Scripted setup | Specify settings or a preset. Use `--dry-run` to inspect, then `--yes` to apply. Bare `setup --yes` no longer silently chooses Ollama or enables LLM extraction. |
| Configuration preview | `config --dry-run` shows search, answers, extraction destination, saved remote routing, changed field names, selected vault, and whether that vault needs reindexing. No settings are saved or models downloaded. `--json` returns a plan with redacted configuration. |
| Configuration display | `config` identifies the shared setting boundary, entity extraction, LLM key status, offline policy, environment remote overrides, and local vault path. `context` shows the actual selected local vault and identity/scope as readable terminal output; `context --json` preserves structured output. |
| Backend check | `config --test` embeds disposable diagnostic text with the saved local embedding setup. It does not open the selected vault or save a memory. It tests embeddings, not answer quality or remote vault health. ONNX can download assets unless offline mode is enabled. |
| Independent model settings | Search backend/model, answer engine, entity extraction, and endpoint credentials have separate controls. `--ollama-api-key none` removes the saved LLM key; environment credentials still apply. Prefer environment credentials to avoid putting secrets in shell history. |
| Model changes | Preview first. Changing embeddings on a populated selected vault requires explicit migration approval. Other vaults are not reindexed by that command. Run `reindex --yes` separately for any vault already using an incompatible index. |
| Recovery | `restore ID_OR_EXACT_TITLE` recovers an archived record with the same ID and attachments, clearing its old expiry. Ambiguous titles fail and require an ID. Permanently deleted records cannot be restored this way. Remote restore ism explicitly unsupported. |
| Configuration reset | `config --reset-db` requires `--yes` and cannot be combined with setting changes/downloads. Invalid options fail before deleting anything. Normal setting changes do not create a local vault. |
| Earlier correctness fixes | Remote commands never silently fall back locally; unsupported options fail explicitly. Unicode titles, timezone/media filters, confidence and TTL round-trip correctly. Precision recall uses the configured answer engine. Daemon scope/session is preserved; secrets are redacted. |

## 1. Build and isolate the review

These commands are for macOS/Linux shells, run from this checkout. The temporary configuration root isolates your normal settings and identities.

```sh
cargo build --locked
export LIGHTMEM_REVIEW_BIN="$PWD/target/debug/lmem"
export LIGHTMEM_REVIEW_DIR="$(mktemp -d)"
export LIGHTMEM_CONFIG_DIR="$LIGHTMEM_REVIEW_DIR/config"
unset LIGHTMEM_CONNECTION LIGHTMEM_DB LIGHTMEM_REMOTE_URL LIGHTMEM_URL
unset LIGHTMEM_API_KEY LIGHTMEM_KEY OLLAMA_API_KEY LMEM_OLLAMA_API_KEY
unset LIGHTMEM_QA_DISABLE
export LIGHTMEM_OFFLINE=1

# This helper always selects your disposable vault.
lmem_review() {
  "$LIGHTMEM_REVIEW_BIN" --db "$LIGHTMEM_REVIEW_DIR/demo.db" "$@"
}

lmem_review --version --json
lmem_review setup --preset demo --dry-run
lmem_review setup --preset demo --yes
lmem_review config
lmem_review context
lmem_review config --test
```

Expected: version 0.8.0; preview clearly says no changes applied; saved mode is hash search, top-1 answers, extraction off. The test returns 384 dimensions without saving anything. Settings apply to all vaults under this temporary configuration root; `--db` chooses the vault, not a separate per-vault configuration file.

Demo mode verifies command behavior. It does not demonstrate neural semantic-search quality. `LIGHTMEM_OFFLINE=1` prevents network-backed model operations; fully cached local ONNX models are required to use ONNX offline.

## 2. Save, find, answer, archive, recover

```sh
lmem_review remember 'The auth service runs on port 8180' \
  --title 'Auth café & API' --tags 'review,auth' --confidence 0.42
lmem_review recall 'auth port'
lmem_review answer 'Which port does auth use?'
lmem_review inspect 'Auth café & API' --json

lmem_review forget 'Auth café & API'
lmem_review list --status expired
lmem_review restore 'Auth café & API'
lmem_review list --status active
```

Expected: saved confidence is 0.42; Unicode title lookup works; recall finds the fact; the answer includes 8180 and its source. Forget archives it. Restore returns the same ID, active again with no old expiry. `forget --hard`, `clear` without `--soft`, and an approved configuration reset permanently delete data; keep these tests disposable.

Verify attachment recovery too:

```sh
printf 'Presentation attachment\n' > "$LIGHTMEM_REVIEW_DIR/sample.txt"
lmem_review remember 'Presentation notes' --title 'Review file' \
  --file "$LIGHTMEM_REVIEW_DIR/sample.txt"
lmem_review list --media document --limit 1 --page 1
lmem_review forget 'Review file'
lmem_review restore 'Review file'
lmem_review inspect 'Review file' --save-to "$LIGHTMEM_REVIEW_DIR/recovered.txt"
cmp "$LIGHTMEM_REVIEW_DIR/sample.txt" "$LIGHTMEM_REVIEW_DIR/recovered.txt"
```

Expected: a document-only page with the correct total; recovery preserves the attachment; `cmp` produces no output and exits successfully.

## 3. Review configuration safety and terminal interaction

```sh
lmem_review config --preset local --dry-run
lmem_review config --preset local --dry-run --json
lmem_review config --preset local --json
lmem_review config --reset-db
lmem_review stats
lmem_review config --entity-extraction off
lmem_review config --reranker top1
lmem_review setup
```

Expected: the preview reports a required hash-to-ONNX migration. Applying with `--json` without approval fails. Reset without `--yes` fails. Counts stay unchanged. In interactive setup select `2` (Demo), inspect the preview, then enter `n`; settings remain unchanged. A bad choice must fail instead of choosing another preset. No browser is required.

Try `lmem_review config --help`, `setup --help`, and `restore --help` for the complete flags. Changing `--url` configures the LLM/embedding endpoint; `config --remote` configures CLI vault routing. They have different purposes. `config --clear-remote` clears saved routing, while `LIGHTMEM_REMOTE_URL`/`LIGHTMEM_URL` must be unset separately.

To inspect the LLM flow without sending data:

```sh
lmem_review config --reranker ollama:qwen2.5:3b \
  --url http://localhost:11434 --entity-extraction off --dry-run
```

Expected: answers use the displayed endpoint, while extraction stays off. Enabling `--entity-extraction llm` explicitly permits remembered text to go to that endpoint during ingestion when offline mode is disabled.

## 4. Connections, backup, and remote CLI

```sh
lmem_review status
lmem_review identity --help
lmem_review connect --list
lmem_review bundle export -o "$LIGHTMEM_REVIEW_DIR/review.zip"
lmem_review bundle info "$LIGHTMEM_REVIEW_DIR/review.zip"
```

Review actual agent installation with `connect PLATFORM` separately when you intend to change that tool's configuration. Capture is explicit: an agent must invoke remember/observe.

For a named connection, use `--connection ID --scope project context` and `--scope agent context` with a registered project connection. The vault path/scope must change accordingly. Do not combine `--connection` with this guide's explicit `--db` helper. See the existing [identity examples](../README.md#identity-aware-connections-v040) to register one.

Use the [remote CLI and backup review steps](review-0.8.0.md#4-verify-remote-behavior) for the earlier API/remote fixes. CLI `restore` is local-only; remote usage must fail without opening a local vault. Bundle restore is available through `bundle import`; review its merge/replace options against a second disposable vault before using it on real data.

## 5. Real models and final acceptance

Once the CLI flow looks right, use the disposable configuration to review actual models:

1. Unset `LIGHTMEM_OFFLINE`; use `lmem_review config --download bge-small` and `config --download minilm-squad2`.
2. Preview `lmem_review config --preset local --dry-run`.
3. Apply `lmem_review config --preset local --yes`; wait for selected-vault migration to finish.
4. Run `lmem_review config --test`, then recall/answer known facts and questions without supporting evidence. Check source selection and abstention.
5. Test your intended Ollama endpoint/model separately if used. Configure extraction independently. Re-enable offline mode after caching to verify local operations work without downloads.

Native Windows/Linux execution and live neural/multimodal quality require separate verification. Automated hash/mock tests do not establish model quality. The HTML changes remain optional reference work. Publishing 0.8.0 still requires your explicit approval after reviewing.

## Validation

New CLI regressions are in `tests/cli_configuration_review.rs`. They cover read-only preview, migration approval, invalid/reset input safety, diagnostics without vault writes, redacted credentials, and archive/file recovery including ambiguous titles.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
LIGHTMEM_QA_DISABLE=1 cargo test --locked --lib --bins --tests
```

Local macOS arm64 verification: **250 Rust tests, 32 Python SDK tests, and 12 utility/documentation tests passed**, plus formatting and strict Clippy. A terminal walkthrough verified setup preview/cancellation, configuration, backend diagnostics, save/recall/answer, archive/recovery, migration refusal, reset input safety, and bundle export/info. The optimized binary also passed setup, diagnostics, 50 writes, sourced answers, recovery, and backup inspection.

For a narrow performance check, 20 separate optimized CLI recall invocations over 50 synthetic records in hash mode measured **6.33 ms p50 / 6.66 ms p95** on this macOS arm64 host, with a warm filesystem. These timings include CLI process startup and do not measure ONNX/LLM inference or establish general performance guarantees. Report any failure with the command, expected result, actual output, and platform. Do not approve the release until your required flows pass.
