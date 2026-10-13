# Layer 4: Controller Test Matrix (`src/controllers/memory_controller.rs`)

Executable Suite: [func_integrity_controller.rs](file:///Users/krishnakanth/Projects/lightmem/tests/func_integrity_controller.rs)

## 1. Lifecycle, Configuration & Index Migration

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::open_default` / `open_at` | `func_controller_open_default_open_at_and_reindex` | `integrity_controller_embedding_migration_guard_and_unknown_backend` | Resolves local/global DB path; validates backend config before opening |
| `LightMem::db_path` / `config` | `func_controller_open_default_open_at_and_reindex` | `integrity_controller_embedding_migration_guard_and_unknown_backend` | Exposes active database path and configuration |
| `LightMem::embedding_migration_needed` / `stored_embedding_identity` / `requested_embedding_identity` | `func_controller_open_default_open_at_and_reindex` | `integrity_controller_embedding_migration_guard_and_unknown_backend` | Detects mismatch between stored index identity and active model identity; blocks writes/recalls until `reindex` is executed |
| `LightMem::reindex` | `func_controller_open_default_open_at_and_reindex` | `integrity_controller_embedding_migration_guard_and_unknown_backend` | Builds replacement vector index off-to-the-side with progress callbacks and atomically swaps via optimistic concurrency control |

## 2. Memory Creation & Attachments

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::remember` | `func_controller_open_default_open_at_and_reindex` | `integrity_controller_remember_rejects_blank_invalid_confidence_and_deduplicates_files` | Infers category/title when omitted; rejects blank text and invalid/NaN confidence |
| `LightMem::remember_with_options` | `func_controller_remember_options_ttl_wikilinks_and_attachments` | `integrity_controller_remember_rejects_blank_invalid_confidence_and_deduplicates_files` | Applies TTL expiration, custom provenance, conflict detection, `--supersede` auto-merge, and automatic `[[wikilink]]` graph edge creation |
| `LightMem::remember_with_attachments` | `func_controller_remember_options_ttl_wikilinks_and_attachments` | `integrity_controller_remember_rejects_blank_invalid_confidence_and_deduplicates_files` | Attaches binary files, auto-tags media kind (`image`, `audio`, etc.), synthesizes fallback title/summary when text is `None`, and deduplicates identical files passed in the same call by SHA-256 |

## 3. Conflict Detection & Resolution

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::find_conflicts` / `find_conflicts_with_options` | `func_controller_find_conflicts_and_merge_conflict_pair` | `integrity_controller_conflict_merge_transfers_attachments_links_and_retires_older` | Nearest-neighbor semantic + lexical overlap detection; deduplicates symmetric pairs and sorts by descending similarity |
| `LightMem::merge_conflict_pair` | `func_controller_find_conflicts_and_merge_conflict_pair` | `integrity_controller_conflict_merge_transfers_attachments_links_and_retires_older` | Transfers attachments and knowledge graph links from older memory to survivor before soft-retiring older memory (`MemoryStatus::Expired`) |

## 4. Hybrid Recall, Multi-Hop Expansion, Listing & Pagination

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::recall` / `recall_with_date` | `func_controller_recall_expanded_list_pagination_and_get` | `integrity_controller_pagination_bounds_and_recall_validation` | Hybrid BM25 + vector RRF search with optional calendar date filter (`NaiveDate`); rejects blank queries and out-of-range `min_similarity` |
| `LightMem::recall_expanded` | `func_controller_recall_expanded_list_pagination_and_get` | `integrity_controller_pagination_bounds_and_recall_validation` | Expands top candidates by 1 hop across the knowledge graph with `0.8 * score` decay without duplicating already-retrieved IDs |
| `LightMem::count` / `list` | `func_controller_recall_expanded_list_pagination_and_get` | `integrity_controller_pagination_bounds_and_recall_validation` | Accurate active/expired/category filtering |
| `LightMem::list_paginated` / `list_paginated_with_date` / `list_page` / `list_page_with_date` | `func_controller_recall_expanded_list_pagination_and_get` | `integrity_controller_pagination_bounds_and_recall_validation` | Computes `total`, `page`, `total_pages`, and `has_more`; rejects `page == 0`, `per_page == 0`, and integer-overflowing offsets (`usize::MAX`) |
| `LightMem::get` | `func_controller_recall_expanded_list_pagination_and_get` | `integrity_controller_pagination_bounds_and_recall_validation` | Resolves by exact UUID, unambiguous ID prefix, or case-insensitive title |

## 5. Attachment Materialization, Deduplication, Forget & Clear

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::get_attachments_data` / `save_attachments_to` / `save_memories_attachments_to` | `func_controller_attachments_materialize_deduplicate_forget_and_clear` | `integrity_controller_hard_forget_and_clear_cleanup_unreferenced_materialized_files` | Verifies SHA-256 and byte length on every read/export |
| `LightMem::default_attachments_dir` / `materialize_attachments` / `materialize_memories_attachments` | `func_controller_attachments_materialize_deduplicate_forget_and_clear` | `integrity_controller_hard_forget_and_clear_cleanup_unreferenced_materialized_files` | Materializes verified files into `<db_parent>/attachments` and returns canonicalized absolute paths |
| `LightMem::forget` / `clear_all` / `deduplicate` | `func_controller_attachments_materialize_deduplicate_forget_and_clear`, `func_controller_graph_export_import_answer_and_observe` | `integrity_controller_hard_forget_and_clear_cleanup_unreferenced_materialized_files` | Hard delete (`forget(..., true)`) removes materialized cache files ONLY when no other memory references the same SHA-256; `clear_all(true)` purges `<db_parent>/attachments` |

## 6. Knowledge Graph, Export/Import, Reranker/Answer & Observer

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem::link` / `unlink` / `get_links` / `autolink` / `related` / `graph` | `func_controller_graph_export_import_answer_and_observe` | `integrity_controller_atomic_directory_import_rollback_and_unknown_reranker` | Full knowledge graph lifecycle (explicit edges, multi-hop traversal, neighborhood snapshots, and vault-wide autolinking) |
| `LightMem::export_okf` / `export_json` / `import_file` / `import_file_with_enrichment` / `stats` | `func_controller_graph_export_import_answer_and_observe` | `integrity_controller_atomic_directory_import_rollback_and_unknown_reranker` | Pre-validates all files in a directory before writing; a single corrupt record rolls back the entire multi-file import so `0` records are committed |
| `LightMem::build_reranker` / `answer` / `answer_with_reranker` / `answer_with_options` | `func_controller_graph_export_import_answer_and_observe` | `integrity_controller_atomic_directory_import_rollback_and_unknown_reranker` | Supports per-query reranker override; rejects unknown reranker names with clear error |
| `LightMem::observe_and_extract` / `save_observed_candidate` / `save_observed_candidates` | `func_controller_graph_export_import_answer_and_observe` | `integrity_controller_atomic_directory_import_rollback_and_unknown_reranker` | Extracts structured memories from transcripts and indexes them with automatic wikilink resolution |
