import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class FootprintReceiptTests: XCTestCase {
    func testListsWhatChangedBetweenTheScans() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("space16x-receipt-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        try manager.createDirectory(at: root.appendingPathComponent("Build"), withIntermediateDirectories: true)
        try Data(count: 100_000).write(to: root.appendingPathComponent("Build/app.o"))
        try Data(count: 50_000).write(to: root.appendingPathComponent("old|log.txt"))
        try Data(count: 10_000).write(to: root.appendingPathComponent("same.txt"))
        let before = try await DiskScanner.scan(path: root.path)
        try Data(count: 300_000).write(to: root.appendingPathComponent("Build/app.o"))
        try Data(count: 200_000).write(to: root.appendingPathComponent("Build/App.dSYM"))
        try manager.removeItem(at: root.appendingPathComponent("old|log.txt"))
        let after = try await DiskScanner.scan(path: root.path)

        let changes = FootprintReceipt.fileChanges(before: before.root, after: after.root, metric: .logical)
        XCTAssertEqual(changes.map(\.path), ["Build/App.dSYM", "Build/app.o", "old|log.txt"])
        XCTAssertEqual(changes.map(\.delta), [200_000, 200_000, -50_000])
        XCTAssertNil(changes[0].before)
        XCTAssertNil(changes[2].after)

        let comparison = ScanComparison(name: "Monday", baseline: before, date: Date(timeIntervalSince1970: 0))
        let receipt = FootprintReceipt.markdown(
            label: "first Xcode build", comparison: comparison, after: after, afterDate: nil,
            metric: .logical, version: "0.8.0", limit: 2
        )
        XCTAssertTrue(receipt.hasPrefix("# Build Footprint: first Xcode build\n"))
        XCTAssertTrue(receipt.contains("(“Monday”)"))
        XCTAssertTrue(receipt.contains("| After | Unknown time |"))
        XCTAssertTrue(receipt.contains("3 files changed"))
        XCTAssertTrue(receipt.contains("| +200 KB | – | 200 KB | Build/App.dSYM |"))
        XCTAssertTrue(receipt.contains("And 1 smaller changes."))
        XCTAssertFalse(receipt.contains(root.path), "paths are relative")
        XCTAssertTrue(receipt.contains("Made with Space 16X 0.8.0."))
    }

    func testParsesBudgetsAndChecksThemLikeTheCLI() {
        XCTAssertEqual(FootprintReceipt.parseBudget("500 MB"), 500_000_000)
        XCTAssertEqual(FootprintReceipt.parseBudget("2GB"), 2_000_000_000)
        XCTAssertEqual(FootprintReceipt.parseBudget("1.5 g"), 1_500_000_000)
        XCTAssertEqual(FootprintReceipt.parseBudget("750000"), 750_000)
        XCTAssertEqual(FootprintReceipt.parseBudget("0"), 0)
        for invalid in ["", "MB", "big", "-1M", "1..5M", "10X"] {
            XCTAssertNil(FootprintReceipt.parseBudget(invalid), invalid)
        }
        // The Rust CLI's prints_signed_changes_and_a_total checks the same rule.
        XCTAssertTrue(FootprintReceipt.isOverBudget(2_000_000, budget: 1_999_999))
        XCTAssertFalse(FootprintReceipt.isOverBudget(2_000_000, budget: 2_000_000))
        XCTAssertFalse(FootprintReceipt.isOverBudget(-2_000_000, budget: 0))
    }

    func testSaysWhetherTheOperationStayedWithinItsBudget() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("space16x-budget-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        try manager.createDirectory(at: root, withIntermediateDirectories: true)
        let before = try await DiskScanner.scan(path: root.path)
        try Data(count: 300_000).write(to: root.appendingPathComponent("build.o"))
        let after = try await DiskScanner.scan(path: root.path)
        let comparison = ScanComparison(name: "Monday", baseline: before, date: nil)
        let receipt = { (budget: UInt64?) in
            FootprintReceipt.markdown(
                label: "build", comparison: comparison, after: after, afterDate: nil,
                metric: .logical, version: "0.9.0", budget: budget
            )
        }
        XCTAssertTrue(receipt(100_000).contains("**Over budget:** the operation grew 300 KB, 200 KB more than its 100 KB budget."))
        XCTAssertTrue(receipt(500_000).contains("**Within budget:** the operation's 500 KB budget has 200 KB to spare."))
        XCTAssertFalse(receipt(nil).contains("budget"))
    }
}
