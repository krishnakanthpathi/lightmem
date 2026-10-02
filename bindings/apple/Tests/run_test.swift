import Foundation

func run() throws {
    print("=== Testing LightMem Swift Native API ===")
    let tempDb = "/tmp/lightmem_swift_test.db"
    if FileManager.default.fileExists(atPath: tempDb) {
        try? FileManager.default.removeItem(atPath: tempDb)
    }

    let client = try LightMem(dbPath: tempDb, globalDb: false)
    print("✔ Connected to database at:", client.dbPath())

    // 1. Remember password
    let rec = try client.remember(
        content: "ghp_apple_silicon_test_key_999",
        category: "password",
        title: "Apple Deployment Token",
        tags: ["auth", "apple", "ci"],
        confidence: 1.0
    )
    print("✔ Stored [\(rec.category)] \(rec.title) (ID: \(rec.id))")
    assert(rec.category == "password")
    assert(rec.title == "Apple Deployment Token")

    // 2. Recall
    let recalled = try client.recall(
        query: "Deployment Token",
        category: "password",
        asOf: nil,
        limit: 5,
        minSimilarity: nil
    )
    print("✔ Recalled \(recalled.count) results")
    assert(!recalled.isEmpty)
    assert(recalled[0].memory.id == rec.id)

    // 3. Stats
    let stats = try client.stats()
    print("✔ Stats: total=\(stats.totalMemories), active=\(stats.activeMemories)")
    assert(stats.totalMemories == 1)
    assert(stats.activeMemories == 1)

    // 4. Forget
    let forgot = try client.forget(id: rec.id, hard: true)
    print("✔ Deleted memory: \(forgot)")
    assert(forgot)

    // Cleanup
    try? FileManager.default.removeItem(atPath: tempDb)
    print("🎉 Swift Apple SDK verification PASSED 100%!")
}

@main
struct Runner {
    static func main() {
        do {
            try run()
        } catch {
            print("❌ Test failed with error: \(error)")
            exit(1)
        }
    }
}
