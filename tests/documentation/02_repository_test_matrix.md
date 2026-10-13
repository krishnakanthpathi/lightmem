# Layer 2: Repository (`Storage`) — Functionality & Integrity Test Matrix

- **Source Module**: [`src/repositories/sqlite_repo.rs`](../../src/repositories/sqlite_repo.rs)
- **Test Suite**: [`tests/func_integrity_repository.rs`](../func_integrity_repository.rs)
- **Status**: **16 / 16 Passed** (`0.09s`)

---

## 1. Lifecycle, Schema & Migrations

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `Storage::{open, open_in_memory, path}` & `migrate` | `func_storage_open_and_in_memory` | `integrity_storage_open_permissions_future_version_and_legacy_repair` | Enforces `0600` owner-only Unix permissions; enables WAL mode & foreign keys; repairs legacy comma-separated tags and empty `dedup_key`; refuses to open future `user_version > 4`. |

---

## 2. Write, Batch Indexing & Optimistic Concurrency Reindex

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `Storage::insert_memory` | `func_insert_memory_and_indexed_batches` | `integrity_insert_memory_validation_and_batch_rollback` | Rejects blank ID/content or NaN/out-of-range confidence; invalidates `embedding_state` on unverified low-level write. |
| `Storage::{embedding_identity, check_embedding_identity}` | `func_insert_memory_and_indexed_batches` | `integrity_replace_index_rejects_stale_revision_missing_ids_and_nan` | Prevents mixing incompatible embedding models in a non-empty vault. |
| `Storage::{insert_indexed_batch, insert_indexed_with_attachments, insert_indexed_batch_with_attachments}` | `func_insert_memory_and_indexed_batches` | `integrity_insert_memory_validation_and_batch_rollback` | Atomic immediate transaction rolls back all memory/attachment rows if embedding provider fails or returns wrong batch count. |
| `Storage::{index_snapshot, replace_index}` | `func_index_snapshot_and_replace_index` | `integrity_replace_index_rejects_stale_revision_missing_ids_and_nan` | Optimistic concurrency check (`storage_revision`) aborts migration if concurrent writes occurred, if any memory ID is omitted, or if any vector contains NaN/Inf. |

---

## 3. Deduplication, Retrieval, Attachments & Graph

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `duplicate_key`, `duplicate_key_for_record`, `Storage::{deduplicate_and_merge, deduplicate_indexed}` | `func_deduplicate_and_merge_and_duplicate_keys` | `integrity_deduplicate_unindexed_invalidates_embedding_state` | Preserves case/indentation in content key; incorporates sorted attachment SHA-256 digests; unions tags, keeps max confidence, and re-embeds merged card. |
| `Storage::{expire_due_memories, get_memory, get_memories_exact, get_memory_by_id_or_title}` | `func_get_memory_exact_prefix_and_title_and_expire_due` | `integrity_get_memory_rejects_empty_and_ambiguous_prefix` | Automatically expires due TTL memories on read; resolves exact UUID, unique prefix, case-insensitive title, or unique >=3-char title substring; errors on ambiguous ID prefix. |
| `Storage::{get_memory_attachments_data, save_attachments_to, save_memories_attachments_to, copy_attachments_between_memories, is_attachment_sha_referenced}` | `func_attachments_save_copy_and_sha_reference` | `integrity_attachment_sha256_corruption_detection` | Verifies SHA-256 digest and byte length on every read/export; disambiguates colliding filenames across memories; blocks in-place `UPDATE` via `trg_attachments_immutable`. |
| `Storage::{add_link, remove_link, get_links_for_memory, all_links, restore_links, copy_links_between_memories, traverse_multi_hop, get_graph_snapshot, auto_link_memory, autolink_vault}` | `func_graph_links_restore_copy_multihop_and_autolink` | `integrity_graph_rejects_self_links_clamps_weights_and_handles_cycles` | Rejects self-loops (`src == dst`); clamps edge weights to `[0.01, 10.0]`; cycle-safe BFS traversal with visited set and hop clamping `[1, 10]`. |
| `Storage::{count_memories, list_memories_paginated, list_memories, search_bm25, get_candidate_vectors, get_candidate_vectors_checked, forget_memory, clear_all, stats}` | `func_list_paginated_bm25_forget_clear_and_stats` | `integrity_bm25_injection_and_corrupt_vector_blob_rejection` | Sanitizes hostile FTS5 syntax characters; detects misaligned/corrupt vector BLOBs in `get_candidate_vectors_checked`; cascades deletions across FTS, vectors, links, and attachments. |
