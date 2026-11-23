import Foundation
import SpaceCore
@testable import SpacePro
import XCTest

final class HTMLReportTests: XCTestCase {
    private struct Structure: Decodable {
        let heading: String
        let sections: [String]
        let contents: [String]
        let largestFiles: [String]
        let fileTypes: [String]
        let forbidden: [String]
    }

    private static let fixtures = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .appendingPathComponent("fixtures")

    /// The text of each cell opening with `marker` in the section headed `heading`.
    private func cells(_ html: String, _ heading: String, _ marker: String) throws -> [String] {
        let start = try XCTUnwrap(html.range(of: "<h2>\(heading)</h2>"))
        var section = html[start.upperBound...]
        if let next = section.range(of: "<h2>") { section = section[..<next.lowerBound] }
        return section.components(separatedBy: marker).dropFirst().compactMap { cell in
            cell.range(of: "</td>").map { String(cell[..<$0.lowerBound]) }
        }
    }

    /// The Rust CLI's `html::tests::matches_the_shared_structure` checks its page against the
    /// same structure.
    func testMatchesTheSharedStructure() throws {
        let expected = try JSONDecoder().decode(
            Structure.self, from: Data(contentsOf: Self.fixtures.appendingPathComponent("html-report/expected.json"))
        )
        let report = try SavedScan(contentsOf: Self.fixtures.appendingPathComponent("cleanup/scan.json")).report
        let html = HTMLReport.page(report: report, metric: .allocated, excludes: [], olderThanDays: nil, version: "0.9.0")
        XCTAssertTrue(html.hasPrefix("<!doctype html>"))
        XCTAssertTrue(html.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix("</html>"))
        XCTAssertTrue(html.contains("<h1>\(expected.heading)</h1>"))
        let sections = html.components(separatedBy: "<h2>").dropFirst().compactMap { part in
            part.range(of: "</h2>").map { String(part[..<$0.lowerBound]) }
        }
        XCTAssertEqual(sections, expected.sections)
        XCTAssertEqual(Array(try cells(html, "Contents", "<td class=\"p\">").prefix(expected.contents.count)), expected.contents)
        XCTAssertEqual(
            Array(try cells(html, "Largest files", "<td class=\"p\">").prefix(expected.largestFiles.count)),
            expected.largestFiles
        )
        // Each app names files without an extension its own way.
        let types = try cells(html, "File types", "<td>").filter { $0.hasPrefix(".") }
        XCTAssertEqual(Array(types.prefix(expected.fileTypes.count)), expected.fileTypes)
        for forbidden in expected.forbidden {
            XCTAssertFalse(html.contains(forbidden), forbidden)
        }
    }

    func testEscapesNamesAndNotesFilters() {
        XCTAssertEqual(HTMLReport.escape("a<'b'>&\""), "a&lt;&#39;b&#39;&gt;&amp;&quot;")
        let boxes = HTMLReport.layout([600, 600, 400, 300, 200, 200, 100, 0], width: 6, height: 4)
        let total = 2400.0
        for (box, size) in zip(boxes, [600.0, 600, 400, 300, 200, 200, 100, 0]) {
            XCTAssertEqual(box.width * box.height, 24 * size / total, accuracy: 1e-9)
        }
    }

    /// The Rust CLI's `--older-than` tests check the same rule.
    func testKeepsOnlyOldFilesAndTheirFolders() async throws {
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("diskuse16x-older-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: root) }
        for folder in ["mixed", "new"] {
            try manager.createDirectory(at: root.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        let old = Date(timeIntervalSinceNow: -400 * 86_400)
        for (name, date) in [("mixed/old.bin", old), ("mixed/new.bin", Date()), ("new/fresh.bin", Date())] {
            let url = root.appendingPathComponent(name)
            try Data(count: 50_000).write(to: url)
            try manager.setAttributes([.modificationDate: date], ofItemAtPath: url.path)
        }
        let report = try await DiskScanner.scan(path: root.path)
        let filtered = OlderThan.filter(report, cutoff: OlderThan.cutoff(days: 365))
        XCTAssertEqual(filtered.root.children.map(\.name), ["mixed"])
        let mixed = try XCTUnwrap(filtered.root.children.first)
        XCTAssertEqual(mixed.children.map(\.name), ["old.bin"])
        XCTAssertLessThan(filtered.root.allocatedBytes, report.root.allocatedBytes)
        XCTAssertEqual(mixed.itemCount, 1)
        XCTAssertEqual(OlderThan.filter(report, cutoff: OlderThan.cutoff(days: 1000)).root.children.count, 0)
    }
}
