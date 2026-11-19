import Foundation
@testable import SpaceCore
import XCTest

#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

final class DiskScannerTests: XCTestCase {
    #if os(macOS)
    func testStartupVolumeExcludesBackingPathsOnlyWhenVisibleWithinScanScope() throws {
        let scope = "/test/System/.."
        let volume = try XCTUnwrap(StartupVolume(scope: scope, canonicalScope: "/"))
        XCTAssertTrue(volume.isBackingPath(scope + "/System/Volumes/Data/Users"))
        XCTAssertTrue(volume.isBackingPath(scope + "/System/Volumes/Data/System/Library/Caches"))
        XCTAssertFalse(volume.isBackingPath(scope + "/System/Volumes/Data"))
        XCTAssertFalse(volume.isBackingPath(scope + "/System/Volumes/DataBackup/Users"))
        XCTAssertFalse(volume.isBackingPath(scope + "/Users"))
        XCTAssertTrue(volume.isAlias("/Users"))
        XCTAssertTrue(volume.isAlias("/System/Library/Caches"))
        XCTAssertFalse(volume.isAlias("/System/Volumes/Data/.fseventsd"))

        let system = try XCTUnwrap(StartupVolume(scope: "/System", canonicalScope: "/System"))
        XCTAssertTrue(system.isAlias("/System/Library/Caches"))
        XCTAssertFalse(system.isAlias("/Users"))
        XCTAssertFalse(system.isAlias("/SystemBackup/Library"))
        let data = "/System/Volumes/Data"
        XCTAssertNil(StartupVolume(scope: data, canonicalScope: data))
    }

    func testReportsVisiblePathsForFirmlinksOnTheStartupDisk() throws {
        let data = "/System/Volumes/Data"
        var visible = stat()
        var backing = stat()
        guard lstat("/Users", &visible) == 0, lstat(data + "/Users", &backing) == 0,
              visible.st_dev == backing.st_dev, visible.st_ino == backing.st_ino else {
            throw XCTSkip("The startup disk has no APFS firmlink for /Users.")
        }
        let mount = open(data, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard mount >= 0 else { throw XCTSkip("The Data volume mount cannot be opened.") }
        defer { close(mount) }
        XCTAssertEqual(try canonicalDirectoryPath(parent: mount, name: "Users"), "/Users")
        XCTAssertEqual(try canonicalDirectoryPath(parent: mount, name: "."), data)
    }

    func testOrdinaryDirectoryWithStartupVolumeLayoutCountsBothTrees() throws {
        try withFixture { root in
            let users = try makeDirectory("Users", under: root)
            let dataUsers = root.appendingPathComponent("System/Volumes/Data/Users", isDirectory: true)
            try FileManager.default.createDirectory(at: dataUsers, withIntermediateDirectories: true)
            let visible = try makeFile("payload", bytes: 8_192, under: users)
            let backing = try makeFile("payload", bytes: 8_192, under: dataUsers)

            let report = try DiskScanner.scan(path: root.path, preview: { _ in })

            XCTAssertFalse(report.isPartial)
            XCTAssertEqual(report.root.allocatedBytes, try allocatedBytes(of: visible) + allocatedBytes(of: backing))
        }
    }
    #endif

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
            let progress = ScanRecorder<ScanProgress>()
            let report = try DiskScanner.scan(path: root.path, progress: progress.record)

            XCTAssertEqual(report.root.logicalBytes, 3)
            let snapshots = progress.snapshots
            XCTAssertEqual(snapshots.first?.visitedEntries, 1)
            XCTAssertEqual(snapshots.last?.visitedEntries, 4)
            XCTAssertEqual(snapshots.last?.currentPath, root.path)
            XCTAssertEqual(snapshots.map(\.visitedEntries), snapshots.map(\.visitedEntries).sorted())

            let cancellation = ScanRecorder<ScanProgress>()
            XCTAssertThrowsError(try DiskScanner.scan(
                path: root.path, progress: cancellation.record,
                isCancelled: { !cancellation.snapshots.isEmpty }
            )) { error in
                XCTAssertTrue(error is CancellationError)
            }
        }
    }

    func testPreviewShowsTwoLevelsWithoutCountingUnresolvedHardLinks() throws {
        try withFixture { root in
            let first = try makeDirectory("a-first", under: root)
            let boundary = try makeDirectory("nested", under: first)
            let deep = try makeFile("deep", bytes: 8_192, under: boundary)
            let visible = try makeFile("visible", bytes: 4_096, under: first)
            try FileManager.default.linkItem(at: deep, to: root.appendingPathComponent("z-alias"))
            let previews = ScanRecorder<ScanReport>()

            let final = try DiskScanner.scan(path: root.path, preview: previews.record)
            let preview = try XCTUnwrap(previews.snapshots.first)
            let firstFolder = try XCTUnwrap(preview.root.children.first)
            let nested = try XCTUnwrap(firstFolder.children.first)
            XCTAssertTrue(preview.isPreview)
            XCTAssertEqual(nested.path, boundary.path)
            XCTAssertTrue(nested.isPending)
            XCTAssertTrue(nested.children.isEmpty)
            XCTAssertEqual(preview.root.allocatedBytes, try allocatedBytes(of: visible))
            XCTAssertEqual(preview.largestFiles(limit: 10, by: .allocated).map(\.path), [visible.path])
            XCTAssertEqual(preview.root.children.last?.isPending, true)

            let direct = try DiskScanner.scan(path: root.path)
            XCTAssertFalse(final.isPreview)
            XCTAssertFalse(final.root.isPending)
            XCTAssertEqual(final.root.allocatedBytes, direct.root.allocatedBytes)
            XCTAssertEqual(final.root.logicalBytes, direct.root.logicalBytes)
            XCTAssertEqual(final.root.children.last?.isHardLinkAlias, true)
            XCTAssertEqual(final.largestFiles(limit: 10, by: .allocated).map(\.path),
                           direct.largestFiles(limit: 10, by: .allocated).map(\.path))
        }
    }

    func testPreviewUpdatesFinishedFoldersAndKeepsOtherFoldersPending() throws {
        try withFixture { root in
            for name in ["first", "second"] {
                let folder = try makeDirectory(name, under: root)
                let nested = try makeDirectory("nested", under: folder)
                _ = try makeFile("payload", bytes: 10, under: nested)
            }
            let previews = ScanRecorder<ScanReport>()
            _ = try DiskScanner.scan(path: root.path, progress: { update in
                if update.visitedEntries == 1 {
                    // Let the publication interval elapse before any branch completes.
                    Thread.sleep(forTimeInterval: 0.45)
                }
            }, preview: previews.record)

            let refined = try XCTUnwrap(previews.snapshots.dropFirst().first)
            XCTAssertEqual(refined.root.children.first?.children.first?.children.first?.name, "payload")
            XCTAssertEqual(refined.root.children.first?.isPending, false)
            XCTAssertEqual(refined.root.children.last?.isPending, true)
            XCTAssertTrue(refined.isPreview)
        }
    }

    func testCancellationAfterPreviewStopsBeforeDeepScan() throws {
        try withFixture { root in
            let folder = try makeDirectory("first", under: root)
            let nested = try makeDirectory("second", under: folder)
            _ = try makeFile("payload", bytes: 10, under: nested)
            let previews = ScanRecorder<ScanReport>()
            let progress = ScanRecorder<ScanProgress>()

            XCTAssertThrowsError(try DiskScanner.scan(
                path: root.path, progress: progress.record, preview: previews.record,
                isCancelled: { !previews.snapshots.isEmpty }
            )) { error in
                XCTAssertTrue(error is CancellationError)
            }
            XCTAssertEqual(previews.snapshots.count, 1)
            XCTAssertTrue(progress.snapshots.isEmpty)
        }
    }

    #if canImport(Darwin)
    func testCloudMetadataTimeoutDoesNotDelayTheSamePathTwice() throws {
        let access = MetadataAccess(isCancelled: { false })
        let finish = DispatchSemaphore(value: 0)
        let didFinish = DispatchSemaphore(value: 0)
        defer {
            finish.signal()
            _ = didFinish.wait(timeout: .now() + .seconds(2))
        }
        let path = "/Library/Mobile Documents/provider/folder"
        XCTAssertTrue(MetadataAccess.isProviderPath(path))
        XCTAssertTrue(MetadataAccess.isProviderPath("/Users/test/Library/CloudStorage/Provider"))
        XCTAssertFalse(MetadataAccess.isProviderPath("/Users/test/Library/CloudStorageBackup"))

        let start = DispatchTime.now().uptimeNanoseconds
        XCTAssertThrowsError(try access.perform(at: path, isProvider: true) { _ in
            _ = finish.wait(timeout: .now() + .seconds(5))
            didFinish.signal()
            return 0
        }) { error in
            XCTAssertTrue(error is MetadataUnavailable)
        }
        XCTAssertLessThan(DispatchTime.now().uptimeNanoseconds - start, 3_000_000_000)
        XCTAssertThrowsError(try access.perform(at: path, isProvider: true) { _ in
            XCTFail("A timed-out path should not be retried during the scan.")
            return 0
        })
        XCTAssertEqual(try access.perform(at: "/Local", isProvider: false) { _ in 1 }, 1)
    }

    func testDeepTreeCompletesWhenTheOpenFileLimitStartsAtTheLaunchdDefault() throws {
        try withFixture { root in
            // Each traversal level keeps a directory open. Go deeper than the 256-descriptor
            // soft limit that launchd gives GUI apps, while staying under PATH_MAX.
            var deepest = root
            for _ in 0..<300 { deepest = try makeDirectory("d", under: deepest) }
            let leaf = try makeFile("leaf.dat", bytes: 4_096, under: deepest)

            var original = rlimit()
            XCTAssertEqual(getrlimit(RLIMIT_NOFILE, &original), 0)
            defer { _ = setrlimit(RLIMIT_NOFILE, &original) }
            var launchdDefault = original
            launchdDefault.rlim_cur = min(256, original.rlim_cur)
            XCTAssertEqual(setrlimit(RLIMIT_NOFILE, &launchdDefault), 0)

            let report = try DiskScanner.scan(path: root.path)

            XCTAssertFalse(report.isPartial, "Unexpected issues: \(report.issues.prefix(2).map(\.message))")
            XCTAssertEqual(report.root.allocatedBytes, try allocatedBytes(of: leaf))
        }
    }

    func testScanDisablesCloudMaterializationAndRestoresTheThreadPolicy() throws {
        try withFixture { root in
            let kind = IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES
            let previous = getiopolicy_np(kind, IOPOL_SCOPE_THREAD)
            let policies = ScanRecorder<Int32>()
            _ = try DiskScanner.scan(path: root.path, progress: { _ in
                policies.record(getiopolicy_np(kind, IOPOL_SCOPE_THREAD))
            })

            XCTAssertEqual(policies.snapshots, [IOPOL_MATERIALIZE_DATALESS_FILES_OFF, IOPOL_MATERIALIZE_DATALESS_FILES_OFF])
            XCTAssertEqual(getiopolicy_np(kind, IOPOL_SCOPE_THREAD), previous)
        }
    }
    #endif

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

private final class ScanRecorder<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Value] = []

    var snapshots: [Value] {
        lock.withLock { values }
    }

    func record(_ value: Value) {
        lock.withLock { values.append(value) }
    }
}
