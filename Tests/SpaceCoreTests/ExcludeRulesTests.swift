import Foundation
@testable import SpaceCore
import XCTest

final class ExcludeRulesTests: XCTestCase {
    private struct Table: Decodable {
        struct Case: Decodable {
            let patterns: [String]
            let path: String
            let directory: Bool
            let excluded: Bool
        }

        let cases: [Case]
    }

    /// The same table drives `matches_the_shared_pattern_table` in the Rust CLI.
    func testMatchesTheSharedPatternTable() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("fixtures/exclude-patterns.json")
        let table = try JSONDecoder().decode(Table.self, from: Data(contentsOf: fixture))
        XCTAssertFalse(table.cases.isEmpty)
        for testCase in table.cases {
            let rules = try ExcludeRules(patterns: testCase.patterns)
            let relative = Array(testCase.path.utf8)
            let name = Array((testCase.path.split(separator: "/").last.map(String.init) ?? testCase.path).utf8)
            let excluded = rules.matchesAnyType(relative: relative, name: name)
                || (testCase.directory && rules.matchesDirectory(relative: relative, name: name))
            XCTAssertEqual(
                excluded, testCase.excluded,
                "\(testCase.patterns) against \(testCase.path) (directory: \(testCase.directory))"
            )
        }
    }

    func testReadsPatternsOnePerLineSkippingComments() {
        let text = "# Build output\nnode_modules\n\n  \n*.log  \r\n/build/\n"
        XCTAssertEqual(ExcludeRules.patterns(fromLines: text), ["node_modules", "*.log", "/build/"])
    }

    func testRejectsEmptyPatternsAndSkipsBlankOnes() throws {
        XCTAssertThrowsError(try ExcludeRules(patterns: ["/"]))
        XCTAssertThrowsError(try ExcludeRules(patterns: ["//"]))
        let rules = try ExcludeRules(patterns: ["", "  ", "target  "])
        XCTAssertEqual(rules.sources, ["target"])
        XCTAssertTrue(ExcludeRules().isEmpty)
    }
}

final class EntryOrderTests: XCTestCase {
    private func entry(_ name: String, allocated: UInt64, logical: UInt64, items: UInt64 = 0) -> ScanEntry {
        ScanEntry(
            path: "/r/" + name, name: name, isDirectory: items > 0, isHardLinkAlias: false,
            isCloudOnly: false, isPending: false, logicalBytes: logical, allocatedBytes: allocated,
            itemCount: items, children: []
        )
    }

    func testOrdersBySizeItemsAndNameWithPathTies() {
        let entries = [
            entry("b", allocated: 30, logical: 300, items: 3),
            entry("C", allocated: 100, logical: 10),
            entry("a10", allocated: 50, logical: 50, items: 1),
            entry("a9", allocated: 50, logical: 50, items: 1),
        ]
        let names = { (list: [ScanEntry]) in list.map(\.name) }
        XCTAssertEqual(names(entries.ordered(by: .size, ascending: false, metric: .allocated)), ["C", "a10", "a9", "b"])
        XCTAssertEqual(names(entries.ordered(by: .size, ascending: false, metric: .logical)), ["b", "a10", "a9", "C"])
        XCTAssertEqual(names(entries.ordered(by: .items, ascending: false, metric: .allocated)), ["b", "a10", "a9", "C"])
        XCTAssertEqual(names(entries.ordered(by: .name, ascending: true, metric: .allocated)), ["a9", "a10", "b", "C"])
        XCTAssertEqual(names(entries.ordered(by: .name, ascending: false, metric: .allocated)), ["C", "b", "a10", "a9"])
    }
}
