import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class DuplicateFinderTests: XCTestCase {
    /// The Rust CLI's `groups_files_with_the_same_contents` builds the same folder.
    func testGroupsFilesWithTheSameContents() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("space16x-duplicates-\(UUID().uuidString)")
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
}
