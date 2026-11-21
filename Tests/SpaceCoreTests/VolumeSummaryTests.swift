@testable import SpaceCore
import XCTest

final class VolumeSummaryTests: XCTestCase {
    func testKeepsAvailableSpaceWithinTheTotal() {
        let volume = VolumeSummary(name: "Backup", path: "/Volumes/Backup", totalBytes: 1000, availableBytes: 1200)
        XCTAssertEqual(volume.availableBytes, 1000)
        XCTAssertEqual(volume.usedBytes, 0)
        XCTAssertEqual(VolumeSummary(name: "Disk", path: "/", totalBytes: 1000, availableBytes: 250).usedFraction, 0.75)
        XCTAssertEqual(VolumeSummary(name: "Empty", path: "/x", totalBytes: 0, availableBytes: 0).usedFraction, 0)
    }

    #if os(macOS)
    func testListsTheStartupDisk() throws {
        let startup = try XCTUnwrap(VolumeSummary.mounted().first { $0.path == "/" })
        XCTAssertFalse(startup.name.isEmpty)
        XCTAssertGreaterThan(startup.totalBytes, 0)
        XCTAssertLessThanOrEqual(startup.availableBytes, startup.totalBytes)
    }
    #endif
}
