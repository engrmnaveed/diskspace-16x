import Foundation
import SpaceCore
import XCTest

/// Settings in memory, so the tests leave no preference files behind.
private final class MemoryStore: SettingsStore {
    var values: [String: Any] = [:]
    func object(forKey defaultName: String) -> Any? { values[defaultName] }
    func bool(forKey defaultName: String) -> Bool { values[defaultName] as? Bool ?? false }
    func set(_ value: Any?, forKey defaultName: String) { values[defaultName] = value }
}

final class LegacySettingsTests: XCTestCase {
    func testCopiesTheEarlierAppsSettingsOnce() {
        let defaults = MemoryStore()
        defaults.set("apparent", forKey: "sizeMetric")
        let old: [String: Any] = [
            "proFeatures": true, "excludePatterns": "node_modules", "sizeMetric": "allocated",
            "footprintBudgets": ["build": "500M"], "NSWindow Frame main": "0 0 800 600",
        ]

        let copied = LegacySettings.importOnce(from: old, into: defaults)
        // A setting already made in the new app wins, and other apps' keys stay out.
        XCTAssertEqual(Set(copied), ["proFeatures", "excludePatterns", "footprintBudgets"])
        XCTAssertTrue(defaults.bool(forKey: "proFeatures"))
        XCTAssertEqual(defaults.object(forKey: "excludePatterns") as? String, "node_modules")
        XCTAssertEqual(defaults.object(forKey: "sizeMetric") as? String, "apparent")
        XCTAssertNil(defaults.object(forKey: "NSWindow Frame main"))

        // Only the first launch copies, so a later change in the old app stays there.
        defaults.set(nil, forKey: "proFeatures")
        XCTAssertEqual(LegacySettings.importOnce(from: old, into: defaults), [])
        XCTAssertNil(defaults.object(forKey: "proFeatures"))
    }

    func testWithoutTheEarlierAppNothingIsCopied() {
        let defaults = MemoryStore()
        XCTAssertEqual(LegacySettings.importOnce(from: nil, into: defaults), [])
        XCTAssertTrue(defaults.bool(forKey: LegacySettings.importedKey))
    }

    func testUserDefaultsIsAStore() {
        let store: SettingsStore = UserDefaults.standard
        XCTAssertNotNil(store)
    }
}
