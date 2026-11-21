import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class ScanHistoryTests: XCTestCase {
    func testKeepsScansAndListsThemNewestFirst() async throws {
        let manager = FileManager.default
        let base = manager.temporaryDirectory.appendingPathComponent("space16x-history-\(UUID().uuidString)")
        let data = base.appendingPathComponent("data")
        try manager.createDirectory(at: data, withIntermediateDirectories: true)
        defer { try? manager.removeItem(at: base) }
        try Data(count: 5_000).write(to: data.appendingPathComponent("a.bin"))
        let report = try await DiskScanner.scan(path: data.path)
        let history = ScanHistory(directory: base.appendingPathComponent("History"))
        XCTAssertEqual(history.records(), [])
        XCTAssertFalse(manager.fileExists(atPath: history.directory.path))

        let first = try history.save(SavedScan(report: report, excludes: ["*.log"]), version: "test",
                                     at: Date(timeIntervalSince1970: 1_800_000_000))
        let second = try history.save(SavedScan(report: report, excludes: []), version: "test",
                                      at: Date(timeIntervalSince1970: 1_800_000_000))
        XCTAssertEqual(first.file, "20270115T080000Z-data.json")
        XCTAssertEqual(second.file, "20270115T080000Z-data-2.json")
        XCTAssertEqual(first.scope, report.root.path)
        XCTAssertEqual(first.itemCount, 1)

        let attributes = try manager.attributesOfItem(atPath: history.url(for: first).path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        let reopened = try SavedScan(contentsOf: history.url(for: first))
        XCTAssertEqual(reopened.excludes, ["*.log"])
        XCTAssertEqual(reopened.report.root.allocatedBytes, report.root.allocatedBytes)

        // The CLI writes the same index, and a missing file drops its record.
        let index = history.directory.appendingPathComponent("index.jsonl")
        let handle = try FileHandle(forWritingTo: index)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("not json\n{\"file\":\"20270115T090000Z-srv.json\",\"scope\":\"/srv\",\"savedAt\":1800003600,\"allocatedBytes\":1,\"apparentBytes\":1,\"itemCount\":1,\"isPartial\":false}\n".utf8))
        try handle.close()
        XCTAssertEqual(Set(history.records().map(\.file)), [first.file, second.file])
        try Data("[1,2,{},[{\"name\":\"/srv\"}]]".utf8).write(to: history.directory.appendingPathComponent("20270115T090000Z-srv.json"))
        XCTAssertEqual(history.records().first?.scope, "/srv")
        XCTAssertEqual(history.records(of: report.root.path).count, 2)
        try manager.removeItem(at: history.url(for: second))
        XCTAssertEqual(history.records(of: report.root.path), [first])
    }

    func testNamesFilesLikeTheCLI() {
        XCTAssertEqual(ScanHistory.fileName(scope: "/Users/me/My Files", time: 0), "19700101T000000Z-My_Files")
        XCTAssertEqual(ScanHistory.fileName(scope: "/srv/.cache", time: 0), "19700101T000000Z-cache")
        XCTAssertEqual(ScanHistory.fileName(scope: "/", time: 0), "19700101T000000Z-root")
        XCTAssertEqual(ScanHistory.fileName(scope: "/Volumes/Café", time: 0), "19700101T000000Z-Caf__")
    }
}
