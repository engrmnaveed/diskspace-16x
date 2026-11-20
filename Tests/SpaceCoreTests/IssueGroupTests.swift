@testable import SpaceCore
import XCTest

final class IssueGroupTests: XCTestCase {
    private func issues(_ paths: [String]) -> [ScanIssue] {
        paths.map { ScanIssue(path: $0, message: "Opening directory failed (Operation not permitted).", kind: .notPermitted) }
    }

    func testSingleItemFoldersJoinTheirSiblingsUnderACommonFolder() {
        let home = "/Users/person/Library"
        let groups = IssueGroup.grouping(issues([
            home + "/Containers/com.apple.Notes/Data",
            home + "/Containers/com.apple.Stocks/Data",
            home + "/Containers/com.apple.Weather/Data",
            home + "/Group Containers/group.a",
            home + "/Group Containers/group.b",
            "/private/var/db/one",
            "/Library/Bluetooth",
        ]), under: "/")

        XCTAssertEqual(groups.map(\.folder), [home + "/Containers", home + "/Group Containers", "/Library", "/private/var/db"])
        XCTAssertEqual(groups.map(\.issues.count), [3, 2, 1, 1])
        let containers = groups[0]
        XCTAssertEqual(containers.issues.map(containers.relativePath(of:)), [
            "com.apple.Notes/Data", "com.apple.Stocks/Data", "com.apple.Weather/Data",
        ])
    }

    func testGroupsStayInsideTheScanRoot() {
        let root = "/Users/person"
        let groups = IssueGroup.grouping(issues([
            root + "/Library/Mail",
            root + "/Pictures/Photos Library.photoslibrary",
        ]), under: root)

        // Lifting these single-item folders would reach the scan root, so each keeps its own folder.
        XCTAssertEqual(groups.map(\.folder), [root + "/Library", root + "/Pictures"])
    }
}
