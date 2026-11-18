import Foundation
import SpaceCore
import XCTest

#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

final class DiskScannerTests: XCTestCase {
    func testCountsRegularFileMetadataAndKeepsHardLinkOwnershipAtFirstPath() throws {
        try withFixture { root in
            let firstDirectory = try makeDirectory("a-first", under: root)
            let lastDirectory = try makeDirectory("z-last", under: root)
            let original = try makeFile("original.dat", bytes: 3_017, under: lastDirectory)
            let alias = firstDirectory.appendingPathComponent("alias.dat")
            try FileManager.default.linkItem(at: original, to: alias)
            let hidden = try makeFile(".hidden", bytes: 127, under: root)
            XCTAssertEqual(hidden.path.withCString { chmod($0, 0) }, 0, "Metadata should work without reading file contents")
            try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("loop"), withDestinationURL: root)
            try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("linked-file"), withDestinationURL: original)
            let pipe = root.appendingPathComponent("pipe")
            XCTAssertEqual(pipe.path.withCString { mkfifo($0, 0o600) }, 0)

            let report = try DiskScanner.scan(path: root.path)
            let counted = try XCTUnwrap(report.root.children.first { $0.name == "a-first" })
            let uncounted = try XCTUnwrap(report.root.children.first { $0.name == "z-last" })
            let originalEntry = try XCTUnwrap(uncounted.children.first)

            XCTAssertFalse(report.isPartial)
            XCTAssertEqual(report.root.children.map(\.name), [".hidden", "a-first", "z-last"])
            XCTAssertEqual(report.root.logicalBytes, 3_144)
            XCTAssertEqual(report.root.allocatedBytes, try allocatedBytes(of: original) + allocatedBytes(of: hidden))
            XCTAssertEqual(counted.logicalBytes, 3_017)
            XCTAssertEqual(counted.allocatedBytes, try allocatedBytes(of: original))
            XCTAssertEqual(uncounted.logicalBytes, 0)
            XCTAssertEqual(uncounted.allocatedBytes, 0)
            XCTAssertTrue(originalEntry.isHardLinkAlias)
            XCTAssertFalse(try XCTUnwrap(counted.children.first).isHardLinkAlias)
            XCTAssertEqual(report.largestFiles(limit: 20, by: .logical).map(\.name), ["alias.dat", ".hidden"])
            XCTAssertTrue(report.largestFiles(limit: 20, by: .logical, under: uncounted).isEmpty)
        }
    }

    func testSparseFileReportsLogicalAndAllocatedBytesSeparately() throws {
        try withFixture { root in
            let file = try makeFile("sparse.img", bytes: 0, under: root)
            let handle = try FileHandle(forWritingTo: file)
            let logical: UInt64 = 64 * 1_024 * 1_024
            try handle.truncate(atOffset: logical)
            try handle.close()

            let report = try DiskScanner.scan(path: root.path)

            XCTAssertEqual(report.root.logicalBytes, logical)
            XCTAssertEqual(report.root.allocatedBytes, try allocatedBytes(of: file))
            XCTAssertLessThan(report.root.allocatedBytes, logical)
            XCTAssertEqual(report.largestFiles(limit: 1, by: .logical).first?.path, file.path)
            XCTAssertEqual(report.largestFiles(limit: 1, by: .allocated).first?.path, file.path)
        }
    }

    func testRankedFilesRespectLimitsMetricsAndPathTiesAcrossFolders() throws {
        try withFixture { root in
            let subfolder = try makeDirectory("nested", under: root)
            let fixtures = [
                try makeFile("b.dat", bytes: 16, under: root),
                try makeFile("a.dat", bytes: 16, under: root),
                try makeFile("A.dat", bytes: 16, under: subfolder),
                try makeFile("large.dat", bytes: 70_000, under: subfolder),
                try makeFile("mid.dat", bytes: 32, under: root),
                try makeFile("empty.dat", bytes: 0, under: root),
            ]
            let report = try DiskScanner.scan(path: root.path)
            let sortedLogical = [fixtures[3], fixtures[4], fixtures[1], fixtures[0], fixtures[2], fixtures[5]].map(\.path)
            let sortedAllocated = try fixtures.map { ($0.path, try allocatedBytes(of: $0)) }.sorted {
                $0.1 == $1.1 ? $0.0.utf8.lexicographicallyPrecedes($1.0.utf8) : $0.1 > $1.1
            }.map(\.0)

            for limit in -1...8 {
                XCTAssertEqual(report.largestFiles(limit: limit, by: .logical).map(\.path), Array(sortedLogical.prefix(max(0, limit))))
                XCTAssertEqual(report.largestFiles(limit: limit, by: .allocated).map(\.path), Array(sortedAllocated.prefix(max(0, limit))))
            }
            XCTAssertEqual(report.root.rankedChildren(by: .logical).map(\.name), ["nested", "mid.dat", "a.dat", "b.dat", "empty.dat"])
            let nested = try XCTUnwrap(report.root.children.first { $0.name == "nested" })
            XCTAssertEqual(report.largestFiles(limit: 1, by: .logical, under: nested).map(\.path), [fixtures[3].path])
        }
    }

    func testDoesNotChooseASymlinkOrAFileAsRoot() throws {
        try withFixture { root in
            let file = try makeFile("file", bytes: 1, under: root)
            let directory = try makeDirectory("target", under: root)
            let link = root.appendingPathComponent("link")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: directory)

            for path in [file.path, link.path, link.path + "/", root.path + "/missing", "", "bad\0path"] {
                XCTAssertThrowsError(try DiskScanner.scan(path: path)) { error in
                    XCTAssertTrue(error is ScanFailure)
                }
            }
            XCTAssertEqual(try DiskScanner.scan(path: directory.path + "/./").root.path, directory.path)
        }
    }

    func testAcceptsPathComponentsThatBeginWithACombiningMark() throws {
        try withFixture { root in
            let name = "\u{0301}target"
            let directory = try makeDirectory(name, under: root)
            _ = try makeFile("payload", bytes: 1, under: directory)

            let report = try DiskScanner.scan(path: directory.path)

            XCTAssertEqual(report.root.path, directory.path)
            XCTAssertEqual(report.root.name, name)
            XCTAssertEqual(report.root.logicalBytes, 1)
        }
    }

    func testUnreadableDescendantIsPartialAndUnreadableRootIsFatal() throws {
        guard geteuid() != 0 else { throw XCTSkip("Root can read mode-000 directories.") }
        try withFixture { root in
            _ = try makeFile("visible", bytes: 11, under: root)
            let restricted = try makeDirectory("restricted", under: root)
            _ = try makeFile("secret", bytes: 17, under: restricted)
            XCTAssertEqual(restricted.path.withCString { chmod($0, 0) }, 0)
            defer { _ = restricted.path.withCString { chmod($0, 0o700) } }

            let report = try DiskScanner.scan(path: root.path)

            XCTAssertTrue(report.isPartial)
            XCTAssertEqual(report.root.logicalBytes, 11)
            XCTAssertEqual(report.issues.map(\.path), [restricted.path])
            XCTAssertThrowsError(try DiskScanner.scan(path: restricted.path)) { error in
                XCTAssertTrue(error is ScanFailure)
            }
        }
    }

    #if os(Linux)
    func testUndecodablePOSIXFilenameProducesEscapedIssue() throws {
        try withFixture { root in
            _ = try makeFile("visible", bytes: 11, under: root)
            let directory = root.path.withCString { open($0, O_RDONLY | O_DIRECTORY) }
            guard directory >= 0 else {
                XCTFail("Could not open the fixture root.")
                return
            }
            defer { _ = close(directory) }
            let name: [CChar] = [CChar(bitPattern: 0xFF), 0]
            let file = name.withUnsafeBufferPointer { openat(directory, $0.baseAddress!, O_CREAT | O_WRONLY, 0o600) }
            XCTAssertGreaterThanOrEqual(file, 0)
            _ = close(file)
            defer { name.withUnsafeBufferPointer { _ = unlinkat(directory, $0.baseAddress!, 0) } }

            let report = try DiskScanner.scan(path: root.path)

            XCTAssertTrue(report.isPartial)
            XCTAssertEqual(report.root.children.map(\.name), ["visible"])
            XCTAssertEqual(report.root.logicalBytes, 11)
            XCTAssertEqual(report.issues.map(\.path), [root.path + "/%FF"])
        }
    }
    #endif

    func testReportsProgressFromRootThroughFinalCountAndHonorsCancellation() throws {
        try withFixture { root in
            _ = try makeFile("one", bytes: 1, under: root)
            let folder = try makeDirectory("nested", under: root)
            _ = try makeFile("two", bytes: 2, under: folder)
            let progress = ProgressRecorder()
            let report = try DiskScanner.scan(path: root.path, progress: progress.record)

            XCTAssertEqual(report.root.logicalBytes, 3)
            let snapshots = progress.snapshots
            XCTAssertEqual(snapshots.first?.visitedEntries, 1)
            XCTAssertEqual(snapshots.last?.visitedEntries, 4)
            XCTAssertEqual(snapshots.last?.currentPath, root.path)
            XCTAssertEqual(snapshots.map(\.visitedEntries), snapshots.map(\.visitedEntries).sorted())

            let cancellation = ProgressRecorder()
            XCTAssertThrowsError(try DiskScanner.scan(
                path: root.path, progress: cancellation.record,
                isCancelled: { !cancellation.snapshots.isEmpty }
            )) { error in
                XCTAssertTrue(error is CancellationError)
            }
        }
    }

    private func withFixture(_ body: (URL) throws -> Void) throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("space16x-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        try body(folder)
    }

    private func makeDirectory(_ name: String, under folder: URL) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        return url
    }

    private func makeFile(_ name: String, bytes: Int, under folder: URL) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try Data(repeating: 0x2A, count: bytes).write(to: url)
        return url
    }

    private func allocatedBytes(of file: URL) throws -> UInt64 {
        var metadata = stat()
        guard file.path.withCString({ lstat($0, &metadata) }) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        return UInt64(metadata.st_blocks) * 512
    }
}

private final class ProgressRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [ScanProgress] = []

    var snapshots: [ScanProgress] {
        lock.withLock { values }
    }

    func record(_ progress: ScanProgress) {
        lock.withLock { values.append(progress) }
    }
}
