package dev.lightmem

import dev.lightmem.uniffi.lightmem_ffi.LightMem as NativeLightMem
import dev.lightmem.uniffi.lightmem_ffi.FfiMemoryRecord
import dev.lightmem.uniffi.lightmem_ffi.FfiScoredMemory
import dev.lightmem.uniffi.lightmem_ffi.FfiAnswerResult
import dev.lightmem.uniffi.lightmem_ffi.FfiStorageStats
import dev.lightmem.uniffi.lightmem_ffi.LightMemFfiException

typealias MemoryRecord = FfiMemoryRecord
typealias ScoredMemory = FfiScoredMemory
typealias AnswerResult = FfiAnswerResult
typealias StorageStats = FfiStorageStats
typealias LightMemException = LightMemFfiException

class LightMem(dbPath: String? = null, globalDb: Boolean = false) : AutoCloseable {
    private val native: NativeLightMem = NativeLightMem(dbPath, globalDb)

    val dbPath: String
        get() = native.dbPath()

    fun remember(
        content: String,
        category: String? = null,
        title: String? = null,
        tags: List<String> = emptyList(),
        confidence: Float? = null
    ): MemoryRecord = native.remember(content, category, title, tags, confidence)

    fun recall(
        query: String,
        category: String? = null,
        asOf: String? = null,
        limit: UInt = 10u,
        minSimilarity: Float? = null
    ): List<ScoredMemory> = native.recall(query, category, asOf, limit, minSimilarity)

    fun answer(
        question: String,
        needle: Boolean = false,
        category: String? = null,
        asOf: String? = null,
        limit: UInt = 5u
    ): AnswerResult = native.answer(question, needle, category, asOf, limit)

    fun list(
        category: String? = null,
        status: String? = null,
        asOf: String? = null,
        limit: UInt = 50u
    ): List<MemoryRecord> = native.list(category, status, asOf, limit)

    fun forget(id: String, hard: Boolean = false): Boolean = native.forget(id, hard)

    fun stats(): StorageStats = native.stats()

    fun exportOkf(outputPath: String? = null): String = native.exportOkf(outputPath)

    fun importFile(path: String): ULong = native.importFile(path)

    override fun close() {
        native.destroy()
    }
}
