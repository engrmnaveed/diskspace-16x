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

    func testRejectsEmptyPatternsAndSkipsBlankOnes() throws {
        XCTAssertThrowsError(try ExcludeRules(patterns: ["/"]))
        XCTAssertThrowsError(try ExcludeRules(patterns: ["//"]))
        let rules = try ExcludeRules(patterns: ["", "  ", "target  "])
        XCTAssertEqual(rules.sources, ["target"])
        XCTAssertTrue(ExcludeRules().isEmpty)
    }
}
