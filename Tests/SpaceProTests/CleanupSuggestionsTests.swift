import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class CleanupSuggestionsTests: XCTestCase {
    private struct Expected: Decodable {
        struct Suggestion: Decodable {
            let path: String
            let rule: String
            let allocatedBytes: UInt64
        }

        let minimumBytes: UInt64
        let suggestions: [Suggestion]
    }

    private static let fixtures = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .appendingPathComponent("fixtures/cleanup")

    /// The Rust CLI's `matches_the_shared_cleanup_fixture` reads the same scan and expectations.
    func testMatchesTheSharedCleanupFixture() throws {
        let expected = try JSONDecoder().decode(
            Expected.self, from: Data(contentsOf: Self.fixtures.appendingPathComponent("expected.json"))
        )
        XCTAssertEqual(expected.minimumBytes, CleanupSuggestions.smallestBytes)
        let report = try SavedScan(contentsOf: Self.fixtures.appendingPathComponent("scan.json")).report
        let found = CleanupSuggestions.find(in: report, metric: .allocated)
        XCTAssertEqual(found.map(\.entry.path), expected.suggestions.map(\.path))
        XCTAssertEqual(found.map(\.rule.id), expected.suggestions.map(\.rule))
        XCTAssertEqual(found.map(\.entry.allocatedBytes), expected.suggestions.map(\.allocatedBytes))

        let projects = try XCTUnwrap(report.root.children.first { $0.name == "Projects" })
        XCTAssertEqual(
            CleanupSuggestions.find(in: report, under: projects, metric: .allocated).map(\.rule.id),
            ["rust-target", "node-modules"]
        )
        XCTAssertEqual(found.first?.rule.label, "App caches")
    }
}
