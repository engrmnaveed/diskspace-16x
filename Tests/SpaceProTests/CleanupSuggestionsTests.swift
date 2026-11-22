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
        let review: [Suggestion]
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
        let all = CleanupSuggestions.find(in: report, metric: .allocated)
        let found = all.filter { $0.rule.review == nil }
        XCTAssertEqual(found.map(\.entry.path), expected.suggestions.map(\.path))
        XCTAssertEqual(found.map(\.rule.id), expected.suggestions.map(\.rule))
        XCTAssertEqual(found.map(\.entry.allocatedBytes), expected.suggestions.map(\.allocatedBytes))
        // Xcode archives and simulator devices can't be recreated, so they're for review.
        let review = all.filter { $0.rule.review != nil }
        XCTAssertEqual(review.map(\.entry.path), expected.review.map(\.path))
        XCTAssertEqual(review.map(\.rule.id), expected.review.map(\.rule))
        XCTAssertEqual(review.map(\.entry.allocatedBytes), expected.review.map(\.allocatedBytes))
        XCTAssertNil(CleanupSuggestions.reviewWarning(for: found.map(\.entry)))
        XCTAssertEqual(CleanupSuggestions.reviewWarning(for: [review[1].entry]), "Xcode archives can't be recreated.")
        let library = try XCTUnwrap(report.root.children.first { $0.name == "Library" })
        XCTAssertEqual(
            CleanupSuggestions.reviewWarning(for: [library]),
            "Xcode archives and simulator devices can't be recreated."
        )

        let projects = try XCTUnwrap(report.root.children.first { $0.name == "Projects" })
        XCTAssertEqual(
            CleanupSuggestions.find(in: report, under: projects, metric: .allocated).map(\.rule.id),
            ["rust-target", "node-modules"]
        )
        XCTAssertEqual(found.first?.rule.label, "App caches")
    }
}
