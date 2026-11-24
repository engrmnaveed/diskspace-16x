import Foundation

/// The settings of Space 16X, this app's name before 1.0, which kept them under another
/// bundle identifier. The app copies them once, on its first launch as DiskUse16x.
public enum LegacySettings {
    /// The preferences domain of Space 16X.
    public static let domain = "com.16xbrains.space16x"
    /// Set once the settings were copied, so later changes in either app stay apart.
    public static let importedKey = "importedSpace16XSettings"
    /// The app's own settings: excludes and the size metric (SpaceApp), and Pro's switch,
    /// hint, menu-bar monitor, low-space threshold and footprint budgets (SpacePro).
    public static let keys = [
        "excludePatterns", "sizeMetric", "proFeatures", "proHintDismissed",
        "menuBarMonitor", "lowSpaceThresholdGB", "footprintBudgets",
    ]

    /// Copies each setting that `old` has and `defaults` doesn't, the first time only.
    /// Returns the keys it copied.
    @discardableResult
    public static func importOnce(from old: [String: Any]?, into defaults: UserDefaults) -> [String] {
        guard !defaults.bool(forKey: importedKey) else { return [] }
        var copied: [String] = []
        for key in keys {
            guard defaults.object(forKey: key) == nil, let value = old?[key] else { continue }
            defaults.set(value, forKey: key)
            copied.append(key)
        }
        defaults.set(true, forKey: importedKey)
        return copied
    }
}
