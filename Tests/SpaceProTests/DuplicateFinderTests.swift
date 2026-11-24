import Foundation
@testable import SpaceCore
@testable import SpacePro
import XCTest

final class DuplicateFinderTests: XCTestCase {
    /// The Rust CLI's `groups_files_with_the_same_contents` builds the same folder.
    func testGroupsFilesWithTheSameContents() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("diskuse16x-duplicates-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        try manager.createDirectory(at: root.appendingPathComponent("b"), withIntermediateDirectories: true)
        let big = Data((0..<200_000).map { UInt8($0 * 7 % 251) })
        var late = big
        late[late.count - 1] ^= 1
        let small = Data(repeating: 9, count: 40_000)
        for (name, contents) in [
            ("a.bin", big), ("b/copy.bin", big), ("b/z.bin", big), ("late.bin", late),
            ("s1", small), ("b/s2", small), ("tiny1", Data(repeating: 1, count: 10)),
            ("tiny2", Data(repeating: 1, count: 10)),
        ] {
            try contents.write(to: root.appendingPathComponent(name))
        }
        try manager.linkItem(at: root.appendingPathComponent("s1"), to: root.appendingPathComponent("s1-link"))
        let report = try await DiskScanner.scan(path: root.path)
        let (groups, unreadable) = DuplicateFinder.find(in: report, minimumBytes: 1_000)
        XCTAssertEqual(unreadable, [])
        let prefix = report.root.path + "/"
        XCTAssertEqual(
            groups.map { $0.files.map { String($0.path.dropFirst(prefix.count)) } },
            [["a.bin", "b/copy.bin", "b/z.bin"], ["b/s2", "s1"]]
        )
        guard groups.count == 2 else { return XCTFail("\(groups.map { $0.files.map(\.path) })") }
        XCTAssertEqual(groups[0].length, 200_000)
        XCTAssertGreaterThanOrEqual(groups[0].extraBytes, 400_000)
        XCTAssertTrue(DuplicateFinder.find(in: report, minimumBytes: 300_000).groups.isEmpty)
    }

    /// The Rust CLI's `counts_only_what_removing_clones_frees` builds the same folder: a.bin,
    /// a separate copy b.bin, and c.bin, an APFS clone of a.bin.
    func testCountsOnlyWhatRemovingClonesFrees() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("diskuse16x-duplicate-clones-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        try manager.createDirectory(at: root, withIntermediateDirectories: true)
        let contents = Data((0..<300_000).map { UInt8($0 * 13 % 251) })
        try contents.write(to: root.appendingPathComponent("a.bin"))
        try contents.write(to: root.appendingPathComponent("b.bin"))
        let a = root.appendingPathComponent("a.bin").path
        // The temporary directory may not be on APFS.
        guard clonefile(a, root.appendingPathComponent("c.bin").path, 0) == 0 else { return }
        let report = try await DiskScanner.scan(path: root.path)
        let groups = DuplicateFinder.find(in: report, minimumBytes: 1_000).groups
        guard groups.count == 1, groups[0].files.count == 3 else { return XCTFail("\(groups.map { $0.files.map(\.name) })") }
        let group = groups[0]
        XCTAssertEqual(group.files.map(\.name), ["a.bin", "b.bin", "c.bin"])
        XCTAssertEqual(group.files.indices.map { group.isClone(at: $0) }, [true, false, true])
        XCTAssertEqual(group.exclusiveBytes(at: 0), 0)
        // Removing b.bin frees its blocks. Removing a clone frees nothing while the other
        // clone keeps their blocks.
        let copy = group.files[1].allocatedBytes
        XCTAssertGreaterThanOrEqual(copy, 300_000)
        XCTAssertEqual(group.extraBytes, copy)
        // Without b.bin, the clones free nothing.
        XCTAssertEqual(group.keeping { $0.name != "b.bin" }?.extraBytes, 0)
        XCTAssertNil(group.keeping { $0.name == "a.bin" })
    }

    func testCountsEveryCopyButTheLargestWithoutClones() {
        let files = (0..<3).map { index in
            ScanEntry(
                path: "/tmp/\(index)", isDirectory: false, isHardLinkAlias: false, isCloudOnly: false, isPending: false,
                logicalBytes: 4000, allocatedBytes: 4096, itemCount: 0, children: []
            )
        }
        XCTAssertEqual(DuplicateGroup(length: 4000, files: files).extraBytes, 8192)
        XCTAssertEqual(DuplicateGroup(length: 4000, files: files, sharedBytes: [0, 4096, 0]).extraBytes, 8192)
        XCTAssertEqual(DuplicateGroup(length: 4000, files: files, sharedBytes: [4096, 4096, 4096]).extraBytes, 0)
    }
}
