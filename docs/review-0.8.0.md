# Review LightMem 0.8.0

This is a local review candidate on `codex/0.8.0-review`. Version metadata is prepared for 0.8.0; no release tag, GitHub release, PyPI upload, or website deployment has been created. Publish only after your verification and explicit release approval.

The current review focus is the **terminal CLI**. Start with [the CLI review guide](cli-review-0.8.0.md). Browser changes below are retained from the earlier implementation; they are optional and are not required for configuring or using the CLI.

## What changed

- Added `lmem ui`: an authenticated workspace bound to loopback, with a persistent target/vault/scope bar, setup checklist, memory search and sourced answers, add/archive/restore, attachment downloads, named connections, configuration preview, model downloads, reindex jobs, and bundle backup/restore review.
- Invalid or incomplete HTTP clear requests return 400 without changing data. Blank API keys are rejected. Both secret fields are redacted from configuration output. The unauthenticated health response no longer discloses a database path.
- Use matching 0.8.0 client/server versions when verifying these changes. Remote commands fail explicitly when unsupported. They never fall back to a local vault. Local connection/scope selectors are rejected for remote vault operations, and daemon launch preserves scope/session overrides.
- Fixed UTF-8/reserved-character title lookup, positive timezone offsets, filter validation, filtered media pagination, confidence, subsecond TTL, inspection links, and precision recall semantics. Unsupported remote attachment/image/extraction options are rejected before writing.
- LLM entity extraction is off by default and can be enabled explicitly. The local preset disables it. Offline mode blocks LLM operations and model downloads when required cached assets are missing.
- Import enrichment fails explicitly rather than silently doing nothing. `config --download all` now includes both QA models.
- Workspace writes carry the vault that the tab displayed. A stale tab cannot silently write to a different vault after another tab switches context. Configuration changed outside the workspace requires a restart before applying new settings.
- Added request/header/concurrency limits and same-origin browser protection. The workspace loads no analytics or third-party assets.
- Updated the landing page, version labels, Windows installation, remote capability reference, text-only quick start, command search ranking, readable styles, skill export examples, performance claims, and bundle-info examples.
- Release workflows generate archive checksums; installers and SDK fallback downloads verify them. Python extraction rejects traversal and links. Linux wheel tags no longer claim unverified manylinux compatibility. PR/main CI now runs validation.

## 1. Start a disposable review workspace

Run these commands from this checkout. Use the compiled candidate rather than an installed older `lmem`.

```sh
cargo build --locked
export LIGHTMEM_REVIEW_BIN="$PWD/target/debug/lmem"
export LIGHTMEM_REVIEW_DIR="$(mktemp -d)"
export LIGHTMEM_CONFIG_DIR="$LIGHTMEM_REVIEW_DIR/config"
unset LIGHTMEM_CONNECTION LIGHTMEM_DB LIGHTMEM_REMOTE_URL LIGHTMEM_URL
unset LIGHTMEM_API_KEY LIGHTMEM_KEY LIGHTMEM_OFFLINE LIGHTMEM_QA_DISABLE

"$LIGHTMEM_REVIEW_BIN" --version --json
"$LIGHTMEM_REVIEW_BIN" --db "$LIGHTMEM_REVIEW_DIR/demo.db" config \
  --backend hash --reranker top1 --entity-extraction off
"$LIGHTMEM_REVIEW_BIN" --db "$LIGHTMEM_REVIEW_DIR/demo.db" remember \
  "The auth service runs on port 8180" --title "Auth café & API" --tags "review,auth"
"$LIGHTMEM_REVIEW_BIN" --db "$LIGHTMEM_REVIEW_DIR/demo.db" ui
```

Expected: version `0.8.0`, then a local workspace with one memory. Its top bar and Settings page must identify the disposable database. The browser should open automatically; otherwise open the private URL printed in the terminal. The token disappears from the address bar after loading. Keep that URL private. Ctrl+C stops the server.

Hash mode lets you review the UI without downloading models. It is a deterministic demo/keyword backend, not evidence of neural semantic-search quality. Use a fresh terminal or restore your prior environment after reviewing; do not run destructive tests against a personal vault.

## 2. Verify the UI

| Check | Steps | Expected result |
| --- | --- | --- |
| Overview | Open Overview and Settings. | Correct vault path, counts, active embedding/answer settings, and entity mode. No misleading “all models ready” claim. |
| Search | Search `auth port` in Memories. | The auth memory appears; scores are labeled as ranking signals. |
| Sourced answer | Select Ask a question; ask `Which port does auth use?`. | Top-1 returns the stored fact. View source opens its full record. |
| Add memory | Add a new decision with a title containing `café & + # %`. | Saved text is preserved; the list resets to show the new active memory. The destination is shown in the form. |
| Empty/error states | Search for unrelated text; try invalid values or a missing provider. | Understandable results, validation, or an actionable retry/configuration message. Hash similarity can still return weak matches. |
| Files | Attach a small text file, browse Documents, and download it. | Filtered counts/pages are correct and downloaded bytes match the original. Use a small PNG if you want to verify image metadata. |
| Archive/recover | Archive the disposable decision; select Archived; restore it. | Archive excludes it from current recall. Restore makes the same record active again and clears its old expiration. Saved graph links to active neighbors become visible again. |
| Configuration preview | Open Models & setup and preview the current hash/top-1 settings. | Shows the exact target, engines, entity mode, and global-setting boundary before applying. Secret values stay hidden. |
| Apply settings | Apply the reviewed settings. | A job completes; saved settings and the server’s active configuration agree. |
| Backend test | Click Test saved setup. | Reports successful embedding dimensions without storing another memory. This tests search, not answer quality. |
| Model migration | Preview changing hash embeddings to ONNX. Cancel first. | Populated vault requires explicit reindex approval. Cancel leaves configuration/index unchanged. Download and test the real models only when ready. |
| Connections | Create `review-codex`, user `review-user`, agent `coding`, project `demo`, scope Project. Select it. | A separate project vault opens; scope and connection attribution remain visible. Initial vault still contains its original records. |
| Scope | Switch a named connection between Project and Agent. | Counts and path change to the actual selected scope. Saving a memory records that connection/scope. |
| Tool installation | Review a skill/MCP destination. Cancel unless you intend to change the tool’s configuration. | The confirmation names the tool and replacement behavior. After deliberate installation, reload the tool and verify a save/recall cycle. |
| Multiple tabs | Open the workspace in two tabs. Switch vault in one; try a write from the stale other tab. | The stale write is rejected with a refresh instruction; it does not land in the newly selected vault. |
| Keyboard/phone | Use Tab, Enter, Escape, dialogs, and a phone-sized viewport. | Focus stays visible, dialog close/cancel works, controls remain usable, and the page does not scroll horizontally except contained navigation/code. |

## 3. Check backup and restore with disposable data

1. In Backups, download a bundle. Keep this copy.
2. Add a second harmless memory to the same disposable vault.
3. Select the downloaded bundle and click Inspect bundle. Verify creation time, record count, attachment count, and destination path.
4. Choose Merge first. Review the result after the job finishes.
5. For a replacement test, inspect the bundle again and choose Replace. The UI requires typing `REPLACE`. Only proceed if the path still points to your disposable review database. Its contents should return to the backup state.
6. Inspect a bundle, then add another memory before applying restore. Expected: the stale restore preview is rejected; inspect again before proceeding.

The browser upload limit is 40 MiB. Use the CLI for larger bundles. Inspection is a preview, not a complete archive integrity verification; restore performs full validation. Pause other CLI/agent writes to this vault while replacing it. A restored bundle may require matching model configuration/reindexing before recall works.

## 4. Verify remote behavior

Stop the UI, then start an isolated server using the same environment:

```sh
"$LIGHTMEM_REVIEW_BIN" --db "$LIGHTMEM_REVIEW_DIR/remote.db" serve \
  --host 127.0.0.1 --port 8092 --api-key disposable-review-key
```

In a second terminal, set `LIGHTMEM_REVIEW_BIN` to the candidate’s absolute path. Then:

```sh
"$LIGHTMEM_REVIEW_BIN" --remote http://127.0.0.1:8092 \
  --api-key disposable-review-key remember "Auth uses port 8180" \
  --title "Auth café & API" --confidence 0.42 --no-entities --json

"$LIGHTMEM_REVIEW_BIN" --remote http://127.0.0.1:8092 \
  --api-key disposable-review-key inspect "Auth café & API" --json

"$LIGHTMEM_REVIEW_BIN" --remote http://127.0.0.1:8092 \
  --api-key disposable-review-key list --as-of '2000-01-01T00:00:00+05:30' --json

"$LIGHTMEM_REVIEW_BIN" --remote http://127.0.0.1:8092 \
  --api-key disposable-review-key export --json -o should-not-exist.json
```

Expected: confidence `0.42`; correct Unicode/title lookup; zero historical records for the old cutoff; remote export fails and creates no local backup. Remote image/path/open/extraction options and local connection selectors must also fail explicitly.

With this **disposable server only**, verify invalid deletion requests:

```sh
curl -i -H 'Authorization: Bearer disposable-review-key' \
  -H 'Content-Type: application/json' --data '{invalid' \
  http://127.0.0.1:8092/v1/clear
curl -H 'Authorization: Bearer disposable-review-key' http://127.0.0.1:8092/v1/stats
```

Expected: 400, with the record count unchanged. Starting another server with `--api-key '   '` must fail; omitting the key should generate one. Configuration JSON must show `[REDACTED]` for configured keys. Only the explicit `--no-auth` flag disables authentication.

The built-in server uses HTTP without transport encryption. The test above is loopback-only; remote deployment needs an appropriate network/TLS setup. A server serves its selected vault. Remote client identities do not provide multi-user authentication or remote per-request scoped vaults.

## 5. Live model verification before release

The automated regression suite uses deterministic or mock backends. It does not establish actual model quality.

- In the disposable workspace, select Local ONNX settings. Prepare the selected embedding and answer models, preview the changes, approve migration, and wait for success.
- Store several known facts and ask exact questions plus one with no supporting evidence. Check source selection and abstention, not just whether an answer is returned.
- Repeat with your intended Ollama model/endpoint if you use it. Verify authentication, unavailable-provider messages, latency, and the explicit entity-extraction destination.
- To verify offline behavior, cache the models first, stop the workspace, set `LIGHTMEM_OFFLINE=1`, and restart. Local operations should work with cached assets. Missing assets and LLM-backed operations should fail without a network fallback.
- Verify visual/audio/video behavior separately with a compatible model and any required tools. This review did not validate a live multimodal model.

Model downloads can be large. QA assets are TLS-transported and checked for basic size but are not yet pinned to a content digest. Native release downloads verify `SHA256SUMS`; digest verification is not a publisher-signature scheme.

## 6. Automated checks

```sh
unset LIGHTMEM_OFFLINE LIGHTMEM_QA_DISABLE
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
LIGHTMEM_QA_DISABLE=1 cargo test --locked --lib --bins --tests
node --check static/workspace.js
node --check static/docs.js
node --check static/site.js
python3 -m unittest discover -s tests -p 'test_*.py' -v
PYTHONPATH=python/src LMEM_BINARY_PATH="$LIGHTMEM_REVIEW_BIN" \
  LIGHTMEM_OFFLINE=1 python3 -m unittest discover -s python/tests -v
```

Use the isolated configuration from step 1. The added Rust regressions live in `tests/v080_regressions.rs`; download/extraction tests in `python/tests/test_download_integrity.py`; documentation contracts in `tests/test_documentation_contracts.py`.

### Initial browser/package pass on macOS arm64

These results preceded the terminal configuration follow-up. See [the CLI guide](cli-review-0.8.0.md#validation) for the latest source checks. The earlier installed-wheel check does not validate subsequent CLI changes.

- 243 Rust tests, 32 Python SDK tests, and 12 utility/documentation tests passed.
- Formatting, strict Clippy across all targets, JavaScript and shell syntax, and whitespace checks passed.
- The optimized release binary and macOS arm64 wheel built successfully. A fresh virtual environment installed that wheel with no dependency downloads and verified the bundled 0.8.0 binary, save/get/recall/answer, and backup manifest using an isolated offline vault.
- Browser checks covered search and sourced answers, Unicode saves, archive/recovery, configuration preview/apply, model backend testing, connection/scope switching, backup download/inspection, and attachment upload/download with identical bytes. Phone layout had no page overflow; browser console checks were clean. Documentation search, keyboard expansion, and the Windows installer tab were also checked.
- Live neural/multimodal quality, deliberate tool-configuration installation, and Windows/Linux execution remain for the checks above and release CI. These results are not a claim that every possible defect has been eliminated.

## Compatibility and deliberately limited scope

- Old config files load with entity extraction **off**. Enable it explicitly if you relied on automatic LLM extraction. Ollama presets opt into their documented LLM behavior.
- `LightMemConfig` has a new public `entity_extraction` field. Rust callers using complete struct literals must add it or use `..Default::default()`.
- `recall --precision` uses the configured answer engine instead of forcing ONNX. HTTP recall rejects precision requests; use `/v1/answer` for an answer response.
- Invalid filters and unsupported remote options now produce errors. This intentionally replaces silent fallback/ignored options.
- The management workspace is local-only. Graph relationships are inspectable; a force-directed graph canvas, complete content history, and automatic conversation capture are not included.
- Linux wheels are tagged `linux_x86_64`, built on Ubuntu 22.04, and require platform verification. The previous unverified manylinux tag was removed. macOS Intel still needs source installation; standalone Linux/Windows arm64 targets exist in the release matrix.
- Windows/PowerShell and Linux release artifacts still need native CI/platform verification. Source inspection and macOS tests do not prove they execute correctly there.

## Release approval

After reviewing, report any failures with the step, expected behavior, actual behavior, and a screenshot/error if helpful. When satisfied, explicitly approve publishing **0.8.0**. The release should then use the verified commit, remove review-only copy, pass release CI, create the version tag, and publish the matching artifacts/checksum manifest. Nothing has been published by this change.
