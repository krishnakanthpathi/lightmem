# Layer 1: Models — Functionality & Integrity Test Matrix

- **Source Modules**: [`src/models/config.rs`](../../src/models/config.rs), [`src/models/memory.rs`](../../src/models/memory.rs), [`src/models/link.rs`](../../src/models/link.rs), [`src/models/stats.rs`](../../src/models/stats.rs)
- **Test Suite**: [`tests/func_integrity_models.rs`](../func_integrity_models.rs)
- **Status**: **35 / 35 Passed** (`0.00s`)

---

## 1. `models::config`

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `format_auth_header` | `func_format_auth_header` | `integrity_format_auth_header_idempotency_and_whitespace` | Trims outer whitespace; never double-prefixes `Bearer ` or `bearer `. |
| `LightMemConfig::default` | `func_config_default` | `integrity_config_default_produces_valid_identity` | Default config always yields a deterministic `card-v1:fastembed-7.1.0:Xenova/bge-small-en-v1.5` identity. |
| `LightMemConfig::normalize_reranker` | `func_normalize_reranker` | `integrity_normalize_reranker_legacy_qa_and_fallback` | Normalizes `ollama:<m>`, `openai:<m>`, `llm:<m>`, `onnx:<m>`, `top1`; migrates legacy `qa_model`; unknown strings safely default to `minilm-squad2`. |
| `LightMemConfig::apply_preset` | `func_apply_preset` | `integrity_apply_preset_rejects_unknown_preset` | Configures `local`, `ollama-cloud`, `ollama-local`; returns explicit error on unknown preset names. |
| `LightMemConfig::normalize_ollama_url` & `effective_ollama_url` | `func_normalize_and_effective_ollama_url` | `integrity_normalize_ollama_url_empty_and_whitespace` | Strips `/api/chat`, `/api/generate`, `/api/embeddings`, `/api/embed`, `/api`, `/v1`; empty string defaults to `http://localhost:11434`. |
| `LightMemConfig::effective_ollama_api_key` | `func_effective_ollama_api_key` | `integrity_effective_ollama_api_key_ignores_none_and_blank` | Resolves explicit config key and env vars (`OLLAMA_API_KEY`, `LMEM_OLLAMA_API_KEY`); treats `"none"` and whitespace as `None`. |
| `LightMemConfig::embedding_identity` | `func_embedding_identity_and_summaries` | `integrity_embedding_identity_custom_dir_fingerprints_contents_and_rejects_bad_backend` | Computes FNV-1a 64-bit hash over custom ONNX model/tokenizer files so weight changes trigger reindex; rejects missing dir or unknown backend. |
| `LightMemConfig::{active_embedding_summary, active_reranker_summary}` | `func_embedding_identity_and_summaries` | `func_embedding_identity_and_summaries` | Human-readable summaries reflect active dimensions and endpoint URLs. |
| `LightMemConfig::{config_dir, config_file, global_db_path, save, try_load, load, resolve_db_path}` | `func_config_save_load_and_resolve_db_path` | `integrity_config_corrupt_json_fails_try_load_but_load_falls_back` | Atomic write via UUID temp file + rename; corrupt JSON errors in `try_load` and falls back to `Default` in `load`. |

---

## 2. `models::memory`

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `MemoryType::{as_str, from_str, Display}` & `MemoryStatus::{as_str, from_str}` | `func_memory_type_and_status_roundtrip` | `integrity_memory_type_and_status_reject_invalid_strings` | Bijective mapping across all 14 categories (`passwords` alias supported) and 2 statuses; rejects unknown strings. |
| `MemoryType::infer` | `func_memory_type_infer_all_14_categories` | `integrity_memory_type_infer_prefix_precedence_and_url_token_isolation` | Start-of-line prefix (`Fact:`, `TODO:`, `Error:`) overrides mid-sentence keywords; credential URL check inspects individual whitespace tokens. |
| `compute_sha256_hex` | `func_sha256_hex_nist_vectors` | `integrity_sha256_hex_padding_block_boundaries` | Matches FIPS 180-4 test vectors (`""`, `"abc"`) and 55/56/64-byte padding block boundaries. |
| `encode_base64` & `decode_base64` | `func_base64_encode_decode_roundtrip` | `integrity_base64_decode_whitespace_and_invalid_input` | Exact RFC 4648 round-trip for all mod-3 lengths; ignores ASCII whitespace; rejects invalid length or alphabet bytes. |
| `detect_mime_and_kind`, `parse_image_dimensions`, `parse_audio_duration_secs`, `parse_mp4_dimensions_and_duration` | `func_detect_mime_and_kind_and_media_parsers` | `integrity_media_parsers_and_attachment_input_handle_corrupt_or_spoofed_bytes` | Magic bytes take priority over spoofed file extensions; truncated/corrupt binary headers return `None` without panic. |
| `AttachmentInput::{from_path, from_bytes}` & `AttachmentMetadata::summary_text` | `func_memory_record_and_card_text` | `integrity_media_parsers_and_attachment_input_handle_corrupt_or_spoofed_bytes` | Strips directory traversal (`../../etc/passwd` -> `passwd`); empty filenames default to `attachment.bin`. |
| `MemoryRecord::{new, to_card_text}` | `func_memory_record_and_card_text` | `integrity_memory_record_clamps_out_of_bounds_confidence` | Clamps confidence to `[0.0, 1.0]`; formats category, title, content, attachment summaries, and tags into canonical card text. |
| `parse_ttl_duration` | `func_parse_ttl_duration` | `integrity_parse_ttl_duration_rejects_zero_negative_and_empty` | Supports `ms`, `s`, `m`, `h`, `d`, `w`; rejects `<= 0`, empty, or non-integer values. |
| `extract_meaningful_tokens`, `detect_memory_conflict`, `detect_memory_conflict_with_similarity` | `func_extract_meaningful_tokens_and_conflict_detection` | `integrity_conflict_detection_self_id_and_unrelated_guards` | Self-comparison (`a.id == b.id`) always returns `None`; orders `older_memory` vs `newer_memory` deterministically by `(created_at, id)`. |

---

## 3. `models::link` & `models::stats`

| Function / Method | Functionality Test | Integrity Test | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `MemoryLink::new` & `extract_wikilinks` | `func_memory_link_and_extract_wikilinks` | `integrity_extract_wikilinks_malformed_and_nested_brackets` | Extracts `[[target]]` and `[[target\|label]]`; safely resolves nested `[[outer [[Inner]]` and ignores empty `[[]]`. |
| `StorageStats`, `GraphSnapshot`, `PaginatedMemories` | `func_and_integrity_stats_and_graph_serialization` | `func_and_integrity_stats_and_graph_serialization` | Lossless JSON serialization and count consistency (`active + expired == total`). |
