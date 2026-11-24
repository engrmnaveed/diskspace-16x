import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class ExclusiveSizeTests: XCTestCase {
    /// The Rust CLI's `leaves_out_hard_linked_files` and `leaves_out_blocks_shared_with_clones`
    /// check the same rules.
    func testLeavesOutHardLinksAndClones() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("diskuse16x-exclusive-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        for folder in ["own", "linked"] {
            try manager.createDirectory(at: root.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        try Data(repeating: 1, count: 300_000).write(to: root.appendingPathComponent("own/solo.bin"))
        try Data(repeating: 2, count: 200_000).write(to: root.appendingPathComponent("linked/first.bin"))
        try manager.linkItem(
            at: root.appendingPathComponent("linked/first.bin"), to: root.appendingPathComponent("own/second.bin")
        )
        let report = try await DiskScanner.scan(path: root.path)
        let own = try XCTUnwrap(report.root.children.first { $0.name == "own" })
        let linked = try XCTUnwrap(report.root.children.first { $0.name == "linked" })
        XCTAssertEqual(ExclusiveSize.measure([linked]), 0)
        XCTAssertEqual(ExclusiveSize.measure([own]), own.allocatedBytes)
        XCTAssertEqual(ExclusiveSize.measure([report.root]), own.allocatedBytes)

        // An APFS clone shares all its blocks with the original.
        let solo = root.appendingPathComponent("own/solo.bin")
        guard clonefile(solo.path, root.appendingPathComponent("own/clone.bin").path, 0) == 0 else { return }
        let cloned = try await DiskScanner.scan(path: root.path)
        let file = try XCTUnwrap(cloned.root.children.first { $0.name == "own" }?.children.first { $0.name == "solo.bin" })
        XCTAssertEqual(ExclusiveSize.measure([file]), 0)
        XCTAssertGreaterThan(ExclusiveSize.sharedBytes(atPath: solo.path), 0)
        XCTAssertNil(ExclusiveSize.measure([cloned.root], limit: .zero))
    }

    /// The CLI explorer's `x_measures_exclusive_sizes_after_the_scan` checks the same rules.
    func testMeasuresAWholeScanForTheColumn() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("diskuse16x-exclusive-sizes-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        for folder in ["own", "linked"] {
            try manager.createDirectory(at: root.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        try Data(repeating: 1, count: 300_000).write(to: root.appendingPathComponent("own/solo.bin"))
        try Data(repeating: 2, count: 200_000).write(to: root.appendingPathComponent("linked/first.bin"))
        try manager.linkItem(
            at: root.appendingPathComponent("linked/first.bin"), to: root.appendingPathComponent("own/second.bin")
        )
        let report = try await DiskScanner.scan(path: root.path)
        let reported = ProgressLog()
        let measured = await ExclusiveSizes.measure(report, concurrency: 2) { reported.add($0) }
        let sizes = try XCTUnwrap(measured)
        let own = try XCTUnwrap(report.root.children.first { $0.name == "own" })
        let linked = try XCTUnwrap(report.root.children.first { $0.name == "linked" })
        XCTAssertEqual(sizes.bytes(for: linked), 0)
        XCTAssertEqual(sizes.bytes(for: own), own.allocatedBytes)
        XCTAssertEqual(sizes.bytes(for: report.root), own.allocatedBytes)
        XCTAssertEqual(reported.values.last, 1)
    }
}

/// Progress values reported from the measuring tasks.
private final class ProgressLog: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: [Double] = []
    var values: [Double] { lock.withLock { stored } }
    func add(_ value: Double) { lock.withLock { stored.append(value) } }
}
