import Foundation

/// When the menu-bar monitor warns about low free space: once when free space drops below
/// the threshold, and again only after it has risen 10% above it, so a disk that hovers
/// near the line doesn't post a stream of notifications.
public struct LowSpaceAlert: Sendable, Equatable {
    /// The UserDefaults key behind **Settings → Pro features → Show free space in the menu bar**.
    public static let monitorKey = "menuBarMonitor"
    /// The UserDefaults key for the threshold in gigabytes. Zero turns alerts off.
    public static let thresholdKey = "lowSpaceThresholdGB"
    public static let defaultThresholdGB = 10
    /// The thresholds Settings offers, in gigabytes. Zero turns alerts off.
    public static let thresholdChoices = [0, 5, 10, 20, 50, 100]

    private var alerted = false

    public init() {}

    /// Records the latest free space and returns true when it should post an alert.
    public mutating func update(freeBytes: UInt64, thresholdGB: Int) -> Bool {
        guard thresholdGB > 0 else {
            alerted = false
            return false
        }
        let threshold = UInt64(thresholdGB) * 1_000_000_000
        if freeBytes < threshold {
            defer { alerted = true }
            return !alerted
        }
        if freeBytes >= threshold + threshold / 10 { alerted = false }
        return false
    }
}
