@testable import SpaceCore
import XCTest

final class TrashTests: XCTestCase {
    private func file(_ path: String, _ bytes: UInt64, alias: Bool = false, cloud: Bool = false) -> ScanEntry {
        ScanEntry(
            path: path, name: String(path.split(separator: "/").last!), isDirectory: false,
            isHardLinkAlias: alias, isCloudOnly: cloud, isPending: false,
            logicalBytes: alias ? 0 : bytes * 2, allocatedBytes: alias ? 0 : bytes,
            itemCount: 0, children: []
        )
    }

    private func folder(_ path: String, _ children: [ScanEntry], pending: Bool = false) -> ScanEntry {
        ScanEntry(
            path: path, name: String(path.split(separator: "/").last ?? "/"), isDirectory: true,
            isHardLinkAlias: false, isCloudOnly: false, isPending: pending,
            logicalBytes: children.reduce(0) { $0 + $1.logicalBytes },
            allocatedBytes: children.reduce(0) { $0 + $1.allocatedBytes },
            itemCount: children.reduce(0) { $0 + ($1.isHardLinkAlias ? 0 : $1.itemCount + 1) },
            children: children
        )
    }

    /// big holds two files and a hard-link alias, next to mid and small.
    private func report(issues: [ScanIssue] = []) -> ScanReport {
        let big = folder("/r/big", [file("/r/big/1", 300), file("/r/big/2", 200), file("/r/big/alias", 0, alias: true)])
        return ScanReport(root: folder("/r", [big, file("/r/mid", 100), file("/r/small", 10)]), issues: issues)
    }

    private func child(_ entry: ScanEntry, _ name: String) -> ScanEntry? {
        entry.children.first { $0.name == name }
    }

    func testRemovingEntriesReducesEveryFolderAbove() throws {
        let original = report()
        XCTAssertEqual(original.root.itemCount, 5)

        let updated = original.removing(["/r/big/1", "/r/small"])
        XCTAssertEqual(updated.root.allocatedBytes, 300)
        XCTAssertEqual(updated.root.logicalBytes, 600)
        XCTAssertEqual(updated.root.itemCount, 3)
        let big = try XCTUnwrap(child(updated.root, "big"))
        XCTAssertEqual(big.children.map(\.name), ["2", "alias"])
        XCTAssertEqual(big.allocatedBytes, 200)
        XCTAssertEqual(big.itemCount, 1, "a hard-link alias isn't an item")
        XCTAssertNil(child(updated.root, "small"))
        XCTAssertEqual(child(updated.root, "mid")?.allocatedBytes, 100)

        let withoutAlias = updated.removing(["/r/big/alias"])
        XCTAssertEqual(withoutAlias.root.itemCount, 3)
        XCTAssertEqual(withoutAlias.removing(["/r/big"]).root.itemCount, 1)
        XCTAssertEqual(original.removing([]).root.allocatedBytes, 610)
    }

    func testRemovingDropsSkippedPathsInsideMovedFolders() {
        let issues = [
            ScanIssue(path: "/r/big/locked", message: "Permission denied", kind: .permissionDenied),
            ScanIssue(path: "/r/bigger", message: "Unreadable"),
        ]
        let updated = report(issues: issues).removing(["/r/big"])
        XCTAssertEqual(updated.issues.map(\.path), ["/r/bigger"])
    }

    func testKeepsCloudOnlyUnscannedAndSkippedItems() throws {
        let protected = ScanIssue(path: "/r/big/locked", message: "Permission denied", kind: .permissionDenied)
        let failed = ScanIssue(path: "/r/mid", message: "I/O error")
        let scan = report(issues: [protected, failed])
        let root = scan.root
        XCTAssertEqual(scan.trashBlocker(for: try XCTUnwrap(child(root, "big"))), .protected)
        XCTAssertEqual(scan.trashBlocker(for: try XCTUnwrap(child(root, "mid"))), .incomplete)
        XCTAssertNil(scan.trashBlocker(for: try XCTUnwrap(child(root, "small"))))
        XCTAssertEqual(scan.trashBlocker(for: file("/r/cloud", 0, cloud: true)), .cloudOnly)
        XCTAssertEqual(scan.trashBlocker(for: folder("/r/later", [], pending: true)), .pending)
        // A sibling whose name starts the same isn't inside the skipped path.
        XCTAssertNil(scan.trashBlocker(for: file("/r/mi", 1)))
    }
}
