/**
 * @file lightmem.h
 * @brief LightMem ANSI C / C++ Native API Header
 * @copyright 2026 LightMem Authors
 */

#ifndef LIGHTMEM_H
#define LIGHTMEM_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

/**
 * @brief Opaque handle to an active LightMem database instance.
 */
typedef struct LMemHandle LMemHandle;

/**
 * @brief Get the last error message on the current thread, or NULL if no error.
 * Caller must free the returned string with lmem_free_string.
 */
char* lmem_get_last_error(void);

/**
 * @brief Deallocate a C string returned by LightMem functions.
 */
void lmem_free_string(char* ptr);

/**
 * @brief Open a LightMem database.
 * @param db_path Path to database file, or NULL to use default project (.lightmem.db).
 * @param global_db Set to non-zero to use global user store (~/.lightmem/memories.db).
 * @return Handle pointer on success, NULL on failure.
 */
LMemHandle* lmem_open(const char* db_path, int global_db);

/**
 * @brief Close and deallocate a LightMem database instance.
 */
void lmem_close(LMemHandle* handle);

/**
 * @brief Store a memory fact, decision, preference, or password.
 * @param handle Active LightMem instance.
 * @param content Text content of the memory.
 * @param category Category name (e.g., "fact", "decision", "password"), or NULL.
 * @param title Optional title string, or NULL to auto-generate.
 * @param tags_csv Comma-separated list of tags, or NULL.
 * @param confidence Confidence score (0.0 to 1.0), or negative value for default (0.9).
 * @return JSON-encoded MemoryRecord string on success (free with lmem_free_string), or NULL on failure.
 */
char* lmem_remember(
    LMemHandle* handle,
    const char* content,
    const char* category,
    const char* title,
    const char* tags_csv,
    float confidence
);

/**
 * @brief Recall memories via hybrid BM25 + Vector Cosine Reciprocal Rank Fusion.
 * @param handle Active LightMem instance.
 * @param query Natural language search query.
 * @param category Optional category filter, or NULL.
 * @param as_of Optional historical point-in-time timestamp (YYYY-MM-DD or RFC3339), or NULL.
 * @param limit Maximum results to return (e.g. 10).
 * @param min_similarity Minimum similarity threshold, or negative value to disable.
 * @return JSON array of ScoredMemory objects (free with lmem_free_string), or NULL on failure.
 */
char* lmem_recall(
    LMemHandle* handle,
    const char* query,
    const char* category,
    const char* as_of,
    unsigned int limit,
    float min_similarity
);

/**
 * @brief Synthesize an exact factual answer to a question.
 * @param handle Active LightMem instance.
 * @param question Natural language question.
 * @param needle Set to 1 to enable Needle 3 precision reranker & slot extraction, 0 for instant Top-1.
 * @param category Optional category filter, or NULL.
 * @param as_of Optional historical date filter, or NULL.
 * @param limit Candidate pool limit.
 * @return JSON AnswerResult object (free with lmem_free_string), or NULL on failure.
 */
char* lmem_answer(
    LMemHandle* handle,
    const char* question,
    int needle,
    const char* category,
    const char* as_of,
    unsigned int limit
);

/**
 * @brief List stored memories chronologically.
 * @param handle Active LightMem instance.
 * @param category Optional category filter, or NULL.
 * @param status Filter ("active" or "expired"), or NULL.
 * @param as_of Historical date, or NULL.
 * @param limit Maximum memories to return.
 * @return JSON array of MemoryRecord objects (free with lmem_free_string), or NULL on failure.
 */
char* lmem_list(
    LMemHandle* handle,
    const char* category,
    const char* status,
    const char* as_of,
    unsigned int limit
);

/**
 * @brief Retire or delete a memory.
 * @param handle Active LightMem instance.
 * @param id Full UUID or 8-character prefix of the memory to forget.
 * @param hard Set to 1 for permanent deletion, 0 for soft-retirement.
 * @return 1 on success, 0 on failure.
 */
int lmem_forget(LMemHandle* handle, const char* id, int hard);

/**
 * @brief Retrieve database statistics and breakdown by category.
 * @param handle Active LightMem instance.
 * @return JSON StorageStats object (free with lmem_free_string), or NULL on failure.
 */
char* lmem_stats(LMemHandle* handle);

/**
 * @brief Export memories to an Open Knowledge Format (OKF) markdown bundle.
 * @param handle Active LightMem instance.
 * @param output_path File path to write to, or NULL for auto-naming.
 * @return Output path string (free with lmem_free_string), or NULL on failure.
 */
char* lmem_export_okf(LMemHandle* handle, const char* output_path);

/**
 * @brief Import memories from a JSON or OKF markdown file.
 * @param handle Active LightMem instance.
 * @param file_path File path to import.
 * @return Number of memories imported on success, -1 on failure.
 */
long long lmem_import_file(LMemHandle* handle, const char* file_path);

/**
 * @brief Import memories with optional Needle 3 Action SLM entity extraction.
 * @param handle Active LightMem instance.
 * @param file_path File path to import.
 * @param needle Set to 1 to use Needle 3 extraction, 0 for fast deterministic fallback.
 * @return Number of memories imported on success, -1 on failure.
 */
long long lmem_import_file_needle(LMemHandle* handle, const char* file_path, int needle);

#ifdef __cplusplus
}
#endif

#endif /* LIGHTMEM_H */
