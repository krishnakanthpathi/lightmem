# Layer 5: CLI & View Test Matrix (`src/views/cli_view.rs`, `src/main.rs`)

Executable Suite: [func_integrity_cli_and_view.rs](file:///Users/krishnakanth/Projects/lightmem/tests/func_integrity_cli_and_view.rs)

## 1. CLI View Rendering (`src/views/cli_view.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `CliView::crimson`, `crimson_bold`, `rose`, `violet_bold`, `gold`, `white_bold`, `slate`, `emerald_bold` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | TrueColor 24-bit ANSI formatting |
| `CliView::banner_string`, `render_intro`, `format_path` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Renders 22x38 half-block Crimson Cloud emblem and replaces `$HOME` prefix with `~/` |
| `CliView::render_remembered`, `render_remembered_with_conflicts` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Valid JSON serialization when `json = true`; safe truncation of short IDs (`< 8` chars) and SHA-256 digests (`< 12` chars) |
| `CliView::render_conflict_pair`, `render_conflicts` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Displays active vs resolved conflict pairs; clamps similarity bars to `[0.0, 1.0]` |
| `CliView::render_recall`, `render_paginated_list` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Formats multi-line content, attachment badges, and pagination footers; handles empty result sets cleanly |
| `CliView::render_inspect`, `render_forget`, `render_dedup`, `render_clear` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Renders full memory card, attachments, and knowledge graph links in human & JSON modes |
| `CliView::render_export`, `render_import`, `render_answer`, `render_config`, `render_stats` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Injects computed fields (`vector_count`, `storage_size_bytes`, `embedding_identity`) in JSON mode |
| `CliView::render_link`, `render_unlink`, `render_related`, `render_graph`, `render_autolink` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Renders multi-hop paths and network trees safely |
| `CliView::render_connect`, `render_platforms_list`, `render_observed_card`, `render_observed_candidates`, `render_observed_summary` | `func_cli_view_all_render_methods_human_and_json` | `integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores` | Renders platform deployment status and observer dry-run/ingestion summaries |

## 2. `lmem` CLI Binary Subcommands (`src/main.rs`)

| Subcommand | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `lmem config` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Updates backend/reranker config; refuses silent embedding migration on non-empty DB when non-interactive without `--yes` |
| `lmem remember` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Stores text, `--file` attachments, `--tags`, `--ttl`, and outputs valid `--json`; exits non-zero on blank input without files |
| `lmem recall` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Supports `--limit`, `--json`, and `--path` attachment materialization; exits non-zero on invalid `--min-similarity` |
| `lmem list` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Supports `--page`, `--limit`, `--offset`, and `--json`; exits non-zero on `--page 0` |
| `lmem inspect` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Supports `--json`, `--path`, and `--save-to` attachment export; exits non-zero when memory ID/title does not exist |
| `lmem answer` / `lmem stats` | `func_cli_subcommands_remember_recall_list_inspect_answer_stats` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Synthesizes extractive answer with `--reranker` override and outputs telemetry JSON |
| `lmem link` / `unlink` / `related` / `graph` / `autolink` | `func_cli_subcommands_graph_link_unlink_related_autolink_conflicts_dedup` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Full knowledge graph CLI workflow with `--json` output |
| `lmem conflicts` / `deduplicate` | `func_cli_subcommands_graph_link_unlink_related_autolink_conflicts_dedup` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | Detects and resolves conflicts (`--yes`) and merges duplicates |
| `lmem observe` / `reindex` / `export` / `import` / `connect` / `completions` / `forget` / `clear` | `func_cli_subcommands_observe_export_import_connect_completions_reindex_forget_clear` | `integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration` | End-to-end CLI ingestion, reindexing, backup/restore, agent skill deployment, shell completion generation, and vault cleanup |
