import Foundation
@testable import SpacePro
import XCTest

final class DiskExplanationTests: XCTestCase {
    /// The Rust CLI's `reads_snapshot_names_and_dates` checks the same cases.
    func testReadsSnapshotNamesAndDates() {
        let output = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-11-21-101500.local\n\n"
        XCTAssertEqual(DiskExplanation.parseSnapshots(output), ["com.apple.TimeMachine.2026-11-21-101500.local"])
        XCTAssertEqual(DiskExplanation.parseSnapshots("Snapshots for disk /:\n"), [])
        XCTAssertEqual(
            DiskExplanation.snapshotDate("com.apple.TimeMachine.2026-11-21-101500.local"), "2026-11-21 10:15"
        )
        for other in [
            "com.apple.os.update-ABC",
            "com.apple.TimeMachine.2026-11-21.local",
            "com.apple.TimeMachine.2026-11-21-1015xx.local",
        ] {
            XCTAssertNil(DiskExplanation.snapshotDate(other), other)
        }
    }

    func testFindsTheAPFSContainerOfADevice() {
        XCTAssertEqual(DiskExplanation.container(ofDevice: "/dev/disk3s5"), "disk3")
        XCTAssertEqual(DiskExplanation.container(ofDevice: "/dev/disk3s1s1"), "disk3")
        XCTAssertEqual(DiskExplanation.container(ofDevice: "/dev/disk12s2"), "disk12")
        XCTAssertNil(DiskExplanation.container(ofDevice: "map auto_home"))
        XCTAssertNil(DiskExplanation.container(ofDevice: "/dev/diskX"))
    }

    #if os(macOS)
    func testExplainsTheVolumeOfAFolder() throws {
        let explanation = try XCTUnwrap(DiskExplanation.explain(URL(filePath: NSTemporaryDirectory())))
        XCTAssertGreaterThan(explanation.totalBytes, 0)
        XCTAssertLessThanOrEqual(explanation.usedBytes, explanation.totalBytes)
        if explanation.fileSystem == "apfs" {
            XCTAssertTrue(explanation.containerVolumes.contains { $0.path == explanation.mountPoint })
        }
    }
    #endif
}
