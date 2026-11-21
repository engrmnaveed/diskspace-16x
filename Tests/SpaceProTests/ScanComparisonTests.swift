import SpaceCore
import SpacePro
import XCTest

final class ScanComparisonTests: XCTestCase {
    /// A saved scan read from ncdu JSON, so the test builds trees with public API only.
    private func scan(_ root: String, _ items: String) throws -> ScanReport {
        try SavedScan(data: Data("[1,2,{},[{\"name\":\"\(root)\"},\(items)]]".utf8)).report
    }

    func testMatchesItemsByNameBelowEachRoot() throws {
        let old = try scan("/backup/me", """
        [{"name":"Projects"},{"name":"a.bin","dsize":1000},{"name":"gone.bin","dsize":300}],
        {"name":"keep.txt","dsize":50},{"name":"old.iso","dsize":9000},{"name":"swap","dsize":10}
        """)
        let new = try scan("/Users/me", """
        [{"name":"Projects"},{"name":"a.bin","dsize":4000},{"name":"new.bin","dsize":200}],
        {"name":"keep.txt","dsize":50},[{"name":"swap"}]
        """)
        let comparison = ScanComparison(name: "old.json", baseline: old)
        let top = comparison.changes(in: new.root, at: [], metric: .allocated)
        XCTAssertEqual(top.total, 4250 - 10360)
        XCTAssertEqual(top.children["Projects"], .changed(4200 - 1300))
        XCTAssertEqual(top.children["keep.txt"], .changed(0))
        XCTAssertEqual(top.children["swap"], .added)
        XCTAssertEqual(top.removed.map(\.name), ["old.iso", "swap"])

        let projects = try XCTUnwrap(new.root.children.first { $0.name == "Projects" })
        let inside = comparison.changes(in: projects, at: ["Projects"], metric: .allocated)
        XCTAssertEqual(inside.total, 2900)
        XCTAssertEqual(inside.children["a.bin"], .changed(3000))
        XCTAssertEqual(inside.children["new.bin"], .added)
        XCTAssertEqual(inside.removed.map(\.name), ["gone.bin"])
        let added = try XCTUnwrap(projects.children.first { $0.name == "new.bin" })
        XCTAssertEqual(inside.delta(for: added, metric: .allocated), 200)

        let swap = try XCTUnwrap(new.root.children.first { $0.name == "swap" })
        let missing = comparison.changes(in: swap, at: ["swap"], metric: .allocated)
        XCTAssertNil(missing.total)
        XCTAssertTrue(missing.removed.isEmpty)
    }
}
