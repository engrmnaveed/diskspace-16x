import Foundation
@testable import SpaceCore
import XCTest

final class ModifiedTimeTests: XCTestCase {
    private struct Table: Decodable {
        struct File: Decodable {
            let path: String
            let modified: Int64?
            let hardLinkTo: String?
        }

        let files: [File]
        let folders: [String]
        let newest: [String: UInt32?]
    }

    /// The same table drives `records_each_entrys_newest_modification_time` in the Rust CLI.
    func testRecordsEachEntrysNewestModificationTime() async throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("fixtures/modified-times.json")
        let table = try JSONDecoder().decode(Table.self, from: Data(contentsOf: fixture))
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("diskuse16x-modified-\(UUID().uuidString)")
        let manager = FileManager.default
        try manager.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? manager.removeItem(at: root) }
        for folder in table.folders {
            try manager.createDirectory(at: root.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        for file in table.files {
            let url = root.appendingPathComponent(file.path)
            try manager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let target = file.hardLinkTo {
                try manager.linkItem(at: root.appendingPathComponent(target), to: url)
                continue
            }
            try Data(count: 10).write(to: url)
            var times = [timespec](repeating: timespec(tv_sec: Int(file.modified ?? 0), tv_nsec: 0), count: 2)
            XCTAssertEqual(utimensat(AT_FDCWD, url.path, &times, 0), 0, file.path)
        }

        for concurrency in [1, 4] {
            let report = try await DiskScanner.scan(path: root.path, concurrency: concurrency)
            for (path, expected) in table.newest {
                let entry = try XCTUnwrap(entry(at: path, in: report.root), path)
                XCTAssertEqual(entry.newestModified, expected ?? 0, "\(path) with \(concurrency) tasks")
                XCTAssertEqual(entry.newestModification == nil, expected == nil, path)
            }
        }
    }

    /// The time fills the padding after the flags, so entries take no more memory.
    func testEntriesStayTheSameSize() {
        XCTAssertEqual(MemoryLayout<ScanEntry>.size, 64)
    }

    private func entry(at path: String, in root: ScanEntry) -> ScanEntry? {
        var current = root
        for name in path.split(separator: "/") {
            guard let child = current.children.first(where: { $0.name == name }) else { return nil }
            current = child
        }
        return current
    }
}
