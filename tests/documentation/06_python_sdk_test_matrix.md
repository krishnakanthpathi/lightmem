# Layer 6: Python SDK Test Matrix (`python/src/lmem/*.py`)

Executable Suite: [test_sdk.py](file:///Users/krishnakanth/Projects/lightmem/python/tests/test_sdk.py)

## 1. Binary Discovery & Execution (`python/src/lmem/binary.py`)

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `_is_native_binary` | `test_package_exports` | `test_integrity_binary_validation_and_error_handling` | Verifies Mach-O/ELF/PE magic bytes and executable bit; explicitly rejects `#!` shell/Python wrapper scripts |
| `get_binary_path` / `execute_lmem` | `test_package_exports`, `test_remember_get_and_list` | `test_integrity_binary_validation_and_error_handling` | Resolves `LMEM_BINARY_PATH` / bundled binary; captures `returncode`, `stdout`, `stderr` and raises `RuntimeError` on non-zero exit |

## 2. Python Data Models (`python/src/lmem/models.py`)

| Class / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `AttachmentMetadata.from_dict` / `.media_kind` | `test_attachments_and_paths` | `test_integrity_binary_validation_and_error_handling` | Maps both `media_type` and `media_kind` keys; parses dimensions, size, and SHA-256 |
| `Memory.from_dict` / `MemoryInspection.from_dict` | `test_remember_get_and_list`, `test_inspect_and_deduplicate` | `test_integrity_binary_validation_and_error_handling` | Deserializes nested `memory`, `links`, `attachments`, and `saved_attachments` |
| `ScoredMemory.from_dict` / `AnswerResult.from_dict` / `.evidence` | `test_recall_and_answer` | `test_integrity_binary_validation_and_error_handling` | Exposes `.evidence` from `selected_memory.content` |
| `RelatedMemory.from_dict` / `StorageStats.from_dict` / `PaginatedList` / `ObservedCandidate.from_dict` | `test_linking_and_graph`, `test_observe_conversational_ingestion`, `test_attachments_and_paths` | `test_stats_empty` | Subclasses `list` on `PaginatedList` while preserving `.total`, `.page`, `.has_more`, and `.items` |

## 3. `LightMem` Python Client (`python/src/lmem/client.py`)

| Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `LightMem.remember` | `test_remember_get_and_list`, `test_attachments_and_paths` | `test_integrity_binary_validation_and_error_handling` | Supports text, `files`, `tags`, `confidence`, `provenance`; raises `RuntimeError` on blank content without files or invalid confidence |
| `LightMem.recall` / `LightMem.answer` | `test_recall_and_answer`, `test_attachments_and_paths` | `test_integrity_binary_validation_and_error_handling` | Supports `media` filter and `path=True` attachment materialization |
| `LightMem.list` / `LightMem.get` / `LightMem.inspect` | `test_remember_get_and_list`, `test_inspect_and_deduplicate`, `test_attachments_and_paths` | `test_integrity_binary_validation_and_error_handling` | Returns `None` on missing ID in `get`/`inspect`, and `[]` when `path=True` on missing ID |
| `LightMem.forget` / `LightMem.deduplicate` / `LightMem.conflicts` | `test_forget_and_deduplicate`, `test_inspect_and_deduplicate`, `test_func_connect_and_conflicts` | `test_integrity_binary_validation_and_error_handling` | Soft/hard deletion, deduplication, and nearest-neighbor conflict detection |
| `LightMem.link` / `unlink` / `related` / `graph` / `autolink` | `test_linking_and_graph` | `test_integrity_binary_validation_and_error_handling` | Full knowledge graph traversal and snapshot dictionary generation |
| `LightMem.observe` / `LightMem.connect` / `LightMem.stats` | `test_observe_conversational_ingestion`, `test_func_connect_and_conflicts`, `test_stats_empty` | `test_integrity_binary_validation_and_error_handling` | Returns `ObservedCandidate` list on `dry_run=True` and saved `Memory` list on `yes=True` |
