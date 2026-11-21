import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class FileTypesTests: XCTestCase {
    private struct Table: Decodable {
        struct Case: Decodable {
            let name: String
            let type: String
        }

        let names: [Case]
    }

    /// The same table drives `matches_the_shared_type_table` in the Rust CLI.
    func testMatchesTheSharedTypeTable() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("fixtures/file-types.json")
        let table = try JSONDecoder().decode(Table.self, from: Data(contentsOf: fixture))
        XCTAssertFalse(table.names.isEmpty)
        for testCase in table.names {
            XCTAssertEqual(FileTypes.type(of: testCase.name), testCase.type, testCase.name)
        }
    }

    func testRanksTypesBySizeAndCountsHardLinksOnce() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("space16x-types-\(UUID().uuidString)")
        let manager = FileManager.default
        try manager.createDirectory(at: root.appendingPathComponent("clips"), withIntermediateDirectories: true)
        defer { try? manager.removeItem(at: root) }
        for (name, size) in [("clips/a.MOV", 300_000), ("clips/b.mov", 200_000), ("notes.txt", 9_000), ("Makefile", 5_000)] {
            try Data(count: size).write(to: root.appendingPathComponent(name))
        }
        try manager.linkItem(at: root.appendingPathComponent("clips/a.MOV"), to: root.appendingPathComponent("clips/alias.mov"))
        let report = try await DiskScanner.scan(path: root.path)
        let types = FileTypes.breakdown(of: report.root, metric: .logical)
        XCTAssertEqual(types.map(\.name), ["mov", "txt", ""])
        XCTAssertEqual(types.map(\.fileCount), [2, 1, 1])
        XCTAssertEqual(types[0].logicalBytes, 500_000)
        XCTAssertEqual(types[2].label, "No extension")
        let clips = try XCTUnwrap(report.root.children.first { $0.name == "clips" })
        XCTAssertEqual(FileTypes.breakdown(of: clips, metric: .allocated).map(\.name), ["mov"])
    }
}
