@testable import SpacePro
import XCTest

final class LowSpaceAlertTests: XCTestCase {
    func testAlertsOnceUntilSpaceRecovers() {
        var alert = LowSpaceAlert()
        let gb: UInt64 = 1_000_000_000
        XCTAssertFalse(alert.update(freeBytes: 50 * gb, thresholdGB: 10))
        XCTAssertTrue(alert.update(freeBytes: 9 * gb, thresholdGB: 10))
        XCTAssertFalse(alert.update(freeBytes: 8 * gb, thresholdGB: 10))
        // Hovering just above the line doesn't rearm it.
        XCTAssertFalse(alert.update(freeBytes: 10 * gb + 1, thresholdGB: 10))
        XCTAssertFalse(alert.update(freeBytes: 9 * gb, thresholdGB: 10))
        XCTAssertFalse(alert.update(freeBytes: 11 * gb, thresholdGB: 10))
        XCTAssertTrue(alert.update(freeBytes: 9 * gb, thresholdGB: 10))
        // A threshold of zero turns alerts off.
        XCTAssertFalse(alert.update(freeBytes: 1, thresholdGB: 0))
        XCTAssertTrue(alert.update(freeBytes: 1, thresholdGB: 5))
    }
}
