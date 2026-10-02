import XCTest
@testable import LightMem

final class LightMemTests: XCTestCase {
    func testLifecycle() throws {
        let tempDb = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".db").path
        let client = try LightMem(dbPath: tempDb, globalDb: false)

        // 1. Remember password
        let rec = try client.remember(
            "my_api_key_12345",
            category: "password",
            title: "API Key",
            tags: ["auth"]
        )
        XCTAssertEqual(rec.category, "password")
        XCTAssertEqual(rec.title, "API Key")

        // 2. Recall
        let recalled = try client.recall("API Key", category: "password", limit: 5)
        XCTAssertFalse(recalled.isEmpty)
        XCTAssertEqual(recalled.first?.memory.id, rec.id)

        // 3. Stats
        let stats = try client.stats()
        XCTAssertEqual(stats.totalMemories, 1)

        // 4. Forget
        let forgot = try client.forget(id: rec.id, hard: true)
        XCTAssertTrue(forgot)
    }
}
