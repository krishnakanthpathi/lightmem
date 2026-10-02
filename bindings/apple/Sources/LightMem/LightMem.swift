import Foundation

public typealias MemoryRecord = FfiMemoryRecord
public typealias ScoredMemory = FfiScoredMemory
public typealias AnswerResult = FfiAnswerResult
public typealias StorageStats = FfiStorageStats
public typealias CategoryCount = FfiCategoryCount
public typealias LightMemError = LightMemFfiError

public extension LightMem {
    /// Convenience initializer using default local or global database
    convenience init(global: Bool = false) throws {
        try self.init(dbPath: nil, globalDb: global)
    }

    /// Convenience remember with default tags and confidence
    func remember(
        _ content: String,
        category: String? = nil,
        title: String? = nil,
        tags: [String] = [],
        confidence: Float? = nil
    ) throws -> MemoryRecord {
        try self.remember(content: content, category: category, title: title, tags: tags, confidence: confidence)
    }

    /// Convenience recall with default limit
    func recall(
        _ query: String,
        category: String? = nil,
        asOf: String? = nil,
        limit: UInt32 = 10,
        minSimilarity: Float? = nil
    ) throws -> [ScoredMemory] {
        try self.recall(query: query, category: category, asOf: asOf, limit: limit, minSimilarity: minSimilarity)
    }

    /// Convenience answer
    func answer(
        _ question: String,
        needle: Bool = false,
        category: String? = nil,
        asOf: String? = nil,
        limit: UInt32 = 5
    ) throws -> AnswerResult {
        try self.answer(question: question, needle: needle, category: category, asOf: asOf, limit: limit)
    }
}
