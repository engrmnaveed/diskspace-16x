import Foundation
import SpaceCore
import XCTest

final class LegacySettingsTests: XCTestCase {
    func testCopiesTheEarlierAppsSettingsOnce() throws {
        let suite = "diskuse16x-legacy-settings-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set("apparent", forKey: "sizeMetric")
        let old: [String: Any] = [
            "proFeatures": true, "excludePatterns": "node_modules", "sizeMetric": "allocated",
            "footprintBudgets": ["build": "500M"], "NSWindow Frame main": "0 0 800 600",
        ]

        let copied = LegacySettings.importOnce(from: old, into: defaults)
        // A setting already made in the new app wins, and other apps' keys stay out.
        XCTAssertEqual(Set(copied), ["proFeatures", "excludePatterns", "footprintBudgets"])
        XCTAssertTrue(defaults.bool(forKey: "proFeatures"))
        XCTAssertEqual(defaults.string(forKey: "excludePatterns"), "node_modules")
        XCTAssertEqual(defaults.string(forKey: "sizeMetric"), "apparent")
        XCTAssertNil(defaults.object(forKey: "NSWindow Frame main"))

        // Only the first launch copies, so a later change in the old app stays there.
        defaults.removeObject(forKey: "proFeatures")
        XCTAssertEqual(LegacySettings.importOnce(from: old, into: defaults), [])
        XCTAssertNil(defaults.object(forKey: "proFeatures"))
    }

    func testWithoutTheEarlierAppNothingIsCopied() throws {
        let suite = "diskuse16x-legacy-settings-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        XCTAssertEqual(LegacySettings.importOnce(from: nil, into: defaults), [])
        XCTAssertTrue(defaults.bool(forKey: LegacySettings.importedKey))
    }
}
