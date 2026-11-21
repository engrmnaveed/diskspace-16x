@testable import SpaceCore
import XCTest

/// Mirrors the Rust scanner's rescan tests in `cli/src/scanner/tests.rs`.
final class RescanTests: XCTestCase {
    func testRescanningAFolderMatchesAFullScan() async throws {
        try await withFixture { root in
            try make("outer/inner/old.bin", bytes: 9_000, under: root)
            try make("outer/keep.bin", bytes: 3_000, under: root)
            try make("top.bin", bytes: 5_000, under: root)
            let before = try await DiskScanner.scan(path: root.path)

            try FileManager.default.removeItem(at: root.appendingPathComponent("outer/inner/old.bin"))
            try make("outer/inner/new/a.bin", bytes: 20_000, under: root)
            try make("outer/inner/b.bin", bytes: 700, under: root)
            let inner = root.appendingPathComponent("outer/inner").path
            let subtree = try await DiskScanner.rescan(folder: inner, in: root.path)
            let merged = try XCTUnwrap(before.replacing(subtree))

            assertSameTree(merged.root, try await DiskScanner.scan(path: root.path).root)
            XCTAssertTrue(merged.issues.isEmpty)
            let whole = try await DiskScanner.rescan(folder: root.path, in: root.path)
            assertSameTree(try XCTUnwrap(before.replacing(whole)).root, merged.root)
        }
    }

    func testRescanningKeepsExcludePatternsRelativeToTheScanRoot() async throws {
        try await withFixture { root in
            try make("app/build/out.bin", bytes: 8_000, under: root)
            try make("app/src/build/keep.bin", bytes: 4_000, under: root)
            let excludes = try ExcludeRules(patterns: ["/app/build"])
            let full = try await DiskScanner.scan(path: root.path, excludes: excludes)

            let app = root.appendingPathComponent("app").path
            let subtree = try await DiskScanner.rescan(folder: app, in: root.path, excludes: excludes)
            XCTAssertEqual(subtree.excludedCount, 1)
            let merged = try XCTUnwrap(full.replacing(subtree))
            assertSameTree(merged.root, full.root)
            XCTAssertEqual(merged.excludedCount, 1)
            await assertThrowsAsync(try await DiskScanner.rescan(folder: root.path, in: app, excludes: excludes))
        }
    }

    func testRescanningReplacesSkippedPathsInsideTheFolder() async throws {
        guard geteuid() != 0 else { return }
        try await withFixture { root in
            try make("sub/private/hidden", bytes: 6_000, under: root)
            let other = root.appendingPathComponent("other")
            try FileManager.default.createDirectory(at: other, withIntermediateDirectories: false)
            let privateFolder = root.appendingPathComponent("sub/private")
            try setMode(0, of: privateFolder)
            try setMode(0, of: other)
            defer {
                try? setMode(0o700, of: privateFolder)
                try? setMode(0o700, of: other)
            }
            let before = try await DiskScanner.scan(path: root.path)
            XCTAssertEqual(before.issues.count, 2)

            try setMode(0o700, of: privateFolder)
            let sub = root.appendingPathComponent("sub").path
            let rescanned = try await DiskScanner.rescan(folder: sub, in: root.path)
            let merged = try XCTUnwrap(before.replacing(rescanned))
            XCTAssertEqual(merged.issues.map(\.path), [other.path])
            let folder = try XCTUnwrap(merged.root.children.first { $0.name == "sub" })
            XCTAssertEqual(folder.itemCount, 2)
            XCTAssertEqual(folder.allocatedBytes, try allocatedBytes(of: privateFolder.appendingPathComponent("hidden")))
        }
    }

    func testReplacingNeedsAFolderInTheReport() async throws {
        try await withFixture { root in
            try make("folder/a.bin", bytes: 10, under: root)
            try make("file.bin", bytes: 100, under: root)
            let report = try await DiskScanner.scan(path: root.path)
            let folder = try await DiskScanner.rescan(folder: root.appendingPathComponent("folder").path, in: root.path)
            XCTAssertNil(report.replacing(moved(folder, to: root.appendingPathComponent("gone/folder").path)))
            XCTAssertNil(report.replacing(moved(folder, to: root.appendingPathComponent("file.bin").path)))
            XCTAssertNil(report.replacing(moved(folder, to: "/elsewhere")))
        }
    }

    private func moved(_ report: ScanReport, to path: String) -> ScanReport {
        let root = report.root
        return ScanReport(
            root: ScanEntry(
                path: path, isDirectory: true, isHardLinkAlias: false,
                isCloudOnly: false, isPending: false, logicalBytes: root.logicalBytes,
                allocatedBytes: root.allocatedBytes, itemCount: root.itemCount, children: root.children
            ),
            issues: []
        )
    }

    private func assertSameTree(_ left: ScanEntry, _ right: ScanEntry, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(left.path, right.path, file: file, line: line)
        XCTAssertEqual(left.allocatedBytes, right.allocatedBytes, left.path, file: file, line: line)
        XCTAssertEqual(left.logicalBytes, right.logicalBytes, left.path, file: file, line: line)
        XCTAssertEqual(left.itemCount, right.itemCount, left.path, file: file, line: line)
        XCTAssertEqual(left.isPending, right.isPending, left.path, file: file, line: line)
        let leftChildren = left.children.sorted { $0.path < $1.path }
        let rightChildren = right.children.sorted { $0.path < $1.path }
        XCTAssertEqual(leftChildren.map(\.path), rightChildren.map(\.path), file: file, line: line)
        for (leftChild, rightChild) in zip(leftChildren, rightChildren) {
            assertSameTree(leftChild, rightChild, file: file, line: line)
        }
    }

    private func withFixture(_ body: (URL) async throws -> Void) async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("space16x-rescan-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        try await body(folder)
    }

    private func make(_ relative: String, bytes: Int, under root: URL) throws {
        let url = root.appendingPathComponent(relative)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(repeating: 0x2A, count: bytes).write(to: url)
    }

    private func setMode(_ mode: Int, of url: URL) throws {
        try FileManager.default.setAttributes([.posixPermissions: mode], ofItemAtPath: url.path)
    }

    private func allocatedBytes(of file: URL) throws -> UInt64 {
        var metadata = stat()
        guard file.path.withCString({ lstat($0, &metadata) }) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        return UInt64(metadata.st_blocks) * 512
    }
}
