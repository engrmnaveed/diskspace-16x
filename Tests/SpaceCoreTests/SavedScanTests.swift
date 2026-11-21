import Foundation
@testable import SpaceCore
import XCTest

final class SavedScanTests: XCTestCase {
    private struct Table: Decodable {
        struct Summary: Decodable, Equatable {
            let path: String
            let allocatedBytes: UInt64
            let apparentBytes: UInt64
            let itemCount: UInt64
            var hardLinkAlias: Bool?
            var cloudOnly: Bool?
        }

        struct Issue: Decodable, Equatable {
            let path: String
            let kind: String
        }

        struct Case: Decodable {
            let file: String
            let root: Summary
            let excludedCount: UInt64
            let excludes: [String]
            let issues: [Issue]
            let entries: [Summary]
        }

        let cases: [Case]
    }

    private static let fixtures = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .appendingPathComponent("fixtures/scan-exports")

    private func summary(_ entry: ScanEntry) -> Table.Summary {
        Table.Summary(
            path: entry.path, allocatedBytes: entry.allocatedBytes, apparentBytes: entry.logicalBytes,
            itemCount: entry.itemCount, hardLinkAlias: entry.isHardLinkAlias ? true : nil,
            cloudOnly: entry.isCloudOnly ? true : nil
        )
    }

    /// Every entry below `entry`, depth first.
    private func descendants(_ entry: ScanEntry) -> [ScanEntry] {
        entry.children.flatMap { [$0] + descendants($0) }
    }

    /// The same table drives `matches_the_shared_export_fixtures` in the Rust CLI.
    func testMatchesTheSharedExportFixtures() throws {
        let table = try JSONDecoder().decode(
            Table.self, from: Data(contentsOf: Self.fixtures.appendingPathComponent("expected.json"))
        )
        XCTAssertEqual(table.cases.count, 2)
        for testCase in table.cases {
            let saved = try SavedScan(contentsOf: Self.fixtures.appendingPathComponent(testCase.file))
            let report = saved.report
            XCTAssertEqual(summary(report.root), testCase.root, testCase.file)
            XCTAssertEqual(descendants(report.root).map(summary), testCase.entries, testCase.file)
            XCTAssertEqual(report.excludedCount, testCase.excludedCount, testCase.file)
            XCTAssertEqual(saved.excludes, testCase.excludes, testCase.file)
            XCTAssertEqual(
                report.issues.map { Table.Issue(path: $0.path, kind: $0.kind.rawValue) }, testCase.issues,
                testCase.file
            )
        }
    }

    func testRoundTripsAScan() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("space16x-saved-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        let nested = folder.appendingPathComponent("Nested \"quoted\"\tfolder")
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: false)
        let first = nested.appendingPathComponent("first.bin")
        try Data(repeating: 1, count: 9000).write(to: first)
        try FileManager.default.linkItem(at: first, to: nested.appendingPathComponent("link.bin"))
        try Data(repeating: 2, count: 100).write(to: folder.appendingPathComponent("é.txt"))
        try FileManager.default.createDirectory(
            at: folder.appendingPathComponent("empty"), withIntermediateDirectories: false
        )

        let report = try await DiskScanner.scan(path: folder.path)
        let saved = SavedScan(report: report, excludes: ["*.log"])
        let data = saved.exportData(version: "1.2.3")
        let text = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertTrue(text.hasPrefix("[1,2,{\"progname\":\"space16x\",\"progver\":\"1.2.3\""), text)
        XCTAssertTrue(text.contains("{\"name\":\"Nested \\\"quoted\\\"\\u0009folder\"}"), text)
        XCTAssertNoThrow(try JSONSerialization.jsonObject(with: data))

        let opened = try SavedScan(data: data)
        XCTAssertEqual(summary(opened.report.root), summary(report.root))
        XCTAssertEqual(descendants(opened.report.root).map(summary), descendants(report.root).map(summary))
        XCTAssertEqual(opened.excludes, ["*.log"])
        XCTAssertTrue(opened.report.issues.isEmpty)
        XCTAssertFalse(opened.report.isPreview)
    }

    func testKeepsIssuesAndStatesInItsExports() throws {
        let location = ScanLocation(path: "/data")
        let entry = { (name: String, directory: Bool, alias: Bool, cloud: Bool, bytes: UInt64) in
            ScanEntry(
                name: name, container: location, isDirectory: directory, isHardLinkAlias: alias,
                isCloudOnly: cloud, isPending: false, logicalBytes: bytes, allocatedBytes: bytes,
                itemCount: 0, children: []
            )
        }
        let root = ScanEntry(
            path: "/data", isDirectory: true, isHardLinkAlias: false, isCloudOnly: false, isPending: false,
            logicalBytes: 10, allocatedBytes: 10, itemCount: 4,
            children: [
                entry("kept", false, false, false, 10), entry("alias", false, true, false, 0),
                entry("cloud", true, false, true, 0), entry("locked", true, false, false, 0),
            ]
        )
        let issues = [
            ScanIssue(path: "/data/locked", message: "List failed (Permission denied).", kind: .permissionDenied),
            ScanIssue(path: "/data/vanished", message: "Stat failed (No such file or directory)."),
        ]
        let saved = SavedScan(report: ScanReport(root: root, issues: issues, excludedCount: 2), excludes: [])
        let data = saved.exportData(version: "0")
        let text = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertTrue(text.contains("{\"name\":\"locked\",\"read_error\":true,\"space16x_state\":\"protected\"}"), text)
        XCTAssertTrue(text.contains("{\"name\":\"alias\",\"asize\":0,\"dsize\":0,\"space16x_state\":\"hardLinkAlias\"}"), text)

        let opened = try SavedScan(data: data)
        XCTAssertEqual(opened.report.issues.map(\.path), issues.map(\.path))
        XCTAssertEqual(opened.report.issues.map(\.kind), [.permissionDenied, .error])
        XCTAssertEqual(opened.report.issues.map(\.message), issues.map(\.message))
        XCTAssertEqual(opened.report.excludedCount, 2)
        XCTAssertEqual(opened.report.root.itemCount, 3)
        XCTAssertEqual(opened.report.root.children.map(\.isCloudOnly), [false, false, true, false])
        XCTAssertEqual(opened.report.root.children.map(\.isHardLinkAlias), [false, true, false, false])
    }

    func testRejectsFilesItCannotRead() {
        for (text, message) in [
            ("\u{BF}ncduEX1", "binary export"),
            ("{}", "isn’t an ncdu JSON export"),
            ("[2,0,{},[{\"name\":\"/\"}]]", "format version 2"),
            ("[1,0,{},[{\"name\":\"/\"},{\"name\":\"a\"}", "Line 1: the export ends early."),
            ("[1,0,{},[{\"name\":\"/\"}]]x", "unexpected data"),
            ("[1,0,{},[{\"name\":\"/\"},\n{\"name\":\"a/b\"}]]", "Line 2: an entry has an invalid name."),
            ("[1,0,{},[{\"name\":\"\"}]]", "no name"),
            ("[1,0,{},[{\"name\":\"/\"},{\"name\":\"a\",\"asize\":-1}]]", "whole number"),
            ("[1,0,{},[{\"name\":\"/\"},{\"name\":\"\\x\"}]]", "invalid escape"),
        ] {
            var data = Data(text.utf8)
            if text.hasPrefix("\u{BF}") { data = Data([0xBF]) + Data("ncduEX1".utf8) }
            XCTAssertThrowsError(try SavedScan(data: data), text) { error in
                let description = (error as? SavedScan.ReadError)?.message ?? ""
                XCTAssertTrue(description.contains(message), "\(description) for \(text)")
            }
        }
    }
}
