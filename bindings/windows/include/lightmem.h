/**
 * @file lightmem.h
 * @brief LightMem Windows C / C++ Native API Header
 * @copyright 2026 LightMem Authors
 */

#ifndef LIGHTMEM_WINDOWS_H
#define LIGHTMEM_WINDOWS_H

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) || defined(__CYGWIN__)
  #if defined(LIGHTMEM_EXPORTS)
    #define LIGHTMEM_API __declspec(dllexport)
  #else
    #define LIGHTMEM_API __declspec(dllimport)
  #endif
#else
  #define LIGHTMEM_API
#endif

#include <stddef.h>

typedef struct LMemHandle LMemHandle;

LIGHTMEM_API char* lmem_get_last_error(void);
LIGHTMEM_API void lmem_free_string(char* ptr);

LIGHTMEM_API LMemHandle* lmem_open(const char* db_path, int global_db);
LIGHTMEM_API void lmem_close(LMemHandle* handle);

LIGHTMEM_API char* lmem_remember(
    LMemHandle* handle,
    const char* content,
    const char* category,
    const char* title,
    const char* tags_csv,
    float confidence
);

LIGHTMEM_API char* lmem_recall(
    LMemHandle* handle,
    const char* query,
    const char* category,
    const char* as_of,
    unsigned int limit,
    float min_similarity
);

LIGHTMEM_API char* lmem_answer(
    LMemHandle* handle,
    const char* question,
    int needle,
    const char* category,
    const char* as_of,
    unsigned int limit
);

LIGHTMEM_API char* lmem_list(
    LMemHandle* handle,
    const char* category,
    const char* status,
    const char* as_of,
    unsigned int limit
);

LIGHTMEM_API int lmem_forget(LMemHandle* handle, const char* id, int hard);
LIGHTMEM_API char* lmem_stats(LMemHandle* handle);
LIGHTMEM_API char* lmem_export_okf(LMemHandle* handle, const char* output_path);
LIGHTMEM_API long long lmem_import_file(LMemHandle* handle, const char* file_path);
LIGHTMEM_API long long lmem_import_file_needle(LMemHandle* handle, const char* file_path, int needle);

#ifdef __cplusplus
}
#endif

#endif /* LIGHTMEM_WINDOWS_H */
