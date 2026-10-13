# Layer 3: Services Test Matrix (`src/services/*.rs`)

Executable Suite: [func_integrity_services.rs](file:///Users/krishnakanth/Projects/lightmem/tests/func_integrity_services.rs)

## 1. Embeddings (`src/services/embeddings.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `HashEmbeddingProvider::new` / `default` | `func_hash_embedding_cosine_and_validate_vector` | `integrity_embeddings_validate_vector_and_zero_mismatched_cosine` | Deterministic L2-normalized vector generation; floor dimension enforcement (`dimension.max(16)`) |
| `EmbeddingProvider::embed` / `embed_batch` | `func_hash_embedding_cosine_and_validate_vector`, `func_and_integrity_ollama_embedding_provider_fallback_and_empty_error` | `func_and_integrity_ollama_embedding_provider_fallback_and_empty_error` | Empty/whitespace input rejected with error; unreachable local Ollama seamlessly degrades to `HashEmbeddingProvider` when fallback enabled |
| `EmbeddingProvider::dimension` / `provider_id` / `model_id` | `func_hash_embedding_cosine_and_validate_vector`, `func_and_integrity_ollama_embedding_provider_fallback_and_empty_error` | `integrity_embeddings_validate_vector_and_zero_mismatched_cosine` | Accurate metadata identity (`hash:char-trigram-384`, `ollama:<model>`) |
| `OllamaEmbeddingProvider::new` / `with_fallback` | `func_and_integrity_ollama_embedding_provider_fallback_and_empty_error` | `func_and_integrity_ollama_embedding_provider_fallback_and_empty_error` | Invalid base URL falls back cleanly when `with_fallback` is configured, or errors deterministically when disabled |
| `OnnxEmbeddingProvider::new` / `with_fallback` / `check_onnx_dylib` | `func_hash_embedding_cosine_and_validate_vector` | `integrity_embeddings_validate_vector_and_zero_mismatched_cosine` | Safe dylib presence check without panic; falls back to hash embeddings if ONNX runtime is unavailable |
| `cosine_similarity` | `func_hash_embedding_cosine_and_validate_vector` | `integrity_embeddings_validate_vector_and_zero_mismatched_cosine` | Identical vectors yield `1.0`; mismatched lengths, empty vectors, or zero-norm vectors yield `0.0` without panic or `NaN` |
| `validate_vector` | `func_hash_embedding_cosine_and_validate_vector` | `integrity_embeddings_validate_vector_and_zero_mismatched_cosine` | Rejects empty vectors, `NaN`, and `±INFINITY` values |

## 2. Hybrid Search Engine (`src/services/search.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `HybridSearchEngine::new` | `func_hybrid_search_rrf_and_acronym_boost` | `integrity_hybrid_search_rejects_blank_query_and_invalid_min_similarity` | Clean borrows of `Storage` and `EmbeddingProvider` |
| `HybridSearchEngine::recall` / `recall_with_date` | `func_hybrid_search_rrf_and_acronym_boost` | `integrity_hybrid_search_rejects_blank_query_and_invalid_min_similarity` | Reciprocal Rank Fusion (RRF) combines BM25 lexical rank + cosine vector similarity + confidence + access boosts + acronym expansion (`PostgreSQL (PG)`); rejects blank query, `limit == 0`, and `min_similarity` outside `[0.0, 1.0]`; ignores mismatched-dimension embeddings |

## 3. Reranker & Conflict Merger (`src/services/reranker.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `Top1Reranker::answer` | `func_top1_and_ollama_reranker_answer_and_merge` | `integrity_reranker_low_score_gating_and_hallucination_rejection` | Extractive top-1 selection when confidence threshold is met; returns `found = false` when empty or below `MIN_ANSWER_SCORE` |
| `Top1Reranker::merge_conflict` | `func_top1_and_ollama_reranker_answer_and_merge` | `integrity_reranker_low_score_gating_and_hallucination_rejection` | Deterministic concatenation fallback when no LLM is used |
| `OllamaReranker::new` / `answer` / `merge_conflict` | `func_top1_and_ollama_reranker_answer_and_merge` | `integrity_reranker_low_score_gating_and_hallucination_rejection` | Graceful fallback to extractive / deterministic merge when local Ollama endpoint is offline |
| `PrecisionReranker::new` / `is_extractive_qa_active` / `answer` / `merge_conflict` | `func_top1_and_ollama_reranker_answer_and_merge` | `integrity_reranker_low_score_gating_and_hallucination_rejection` | Respects `LIGHTMEM_QA_DISABLE=1`; enforces post-generation grounding verification against ungrounded hallucinations |

## 4. Observer (`src/services/observer.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `ObserverService::extract` | `func_observer_extract_local_and_candidate_conversion` | `integrity_observer_filters_questions_filler_and_deduplicates` | Classifies preferences, architectural decisions, bugfixes, and facts; strips markdown prefixes; filters out questions (`?`), conversational filler, short fragments, and case-insensitive duplicates |
| `ObservedCandidate::to_memory_record` | `func_observer_extract_local_and_candidate_conversion` | `integrity_observer_filters_questions_filler_and_deduplicates` | Converts candidate to valid `MemoryRecord` with `observer:auto` provenance and sanitized tags |

## 5. Exporter & Importer (`src/services/exporter.rs`, `src/services/importer.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `Exporter::export_json` / `JsonMemoryImporter::load` | `func_exporter_and_importer_json_and_okf_roundtrip` | `integrity_importer_rejects_corrupt_sha256_bad_confidence_and_html_injection` | Lossless JSON serialization and deserialization with inline Base64 attachment payloads; rejects tampered SHA-256 digests and out-of-range confidence |
| `Exporter::export_okf` / `OkfMemoryImporter::load` | `func_exporter_and_importer_json_and_okf_roundtrip` | `integrity_importer_rejects_corrupt_sha256_bad_confidence_and_html_injection` | YAML-frontmatter Markdown (`OKF`) roundtrip preserving `id`, `memory_type`, `status`, `confidence`, `tags`, and `attachments`; rejects HTML (`<!doctype html>`) masquerading as Markdown |

## 6. Platform Connect & Graph View (`src/services/connect.rs`, `src/services/graph_view.rs`)

| Function / Method | Functionality Test (`func_*`) | Integrity Test (`integrity_*`) | Verified Invariants |
| :--- | :--- | :--- | :--- |
| `ConnectService::supported_platforms` / `find_platform` | `func_connect_service_and_graph_view_render_terminal` | `integrity_connect_rejects_unknown_platform_and_graph_view_handles_empty` | Resolves all 6 supported platforms + aliases (`claude`, `agy`, `cursor`, `windsurf`, `opencode`, `codex`); returns `None` for unknown aliases |
| `ConnectService::connect_platform` / `disconnect_platform` / `status_all` | `func_connect_service_and_graph_view_render_terminal` | `integrity_connect_rejects_unknown_platform_and_graph_view_handles_empty` | Idempotent `<!-- LIGHTMEM:START -->` block injection and clean removal without clobbering user content; rejects unknown platforms |
| `render_terminal` | `func_connect_service_and_graph_view_render_terminal` | `integrity_connect_rejects_unknown_platform_and_graph_view_handles_empty` | Renders ASCII/ANSI node degree & edge topology; safely renders empty graph snapshot without division-by-zero |
