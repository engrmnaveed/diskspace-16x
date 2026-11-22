import Foundation

/// Space 16X Pro in the Mac app. Pro analysis lives in this target, which depends on
/// SpaceCore, and the app shows its features only after the user turns them on.
public enum ProFeatures {
    /// The UserDefaults key behind **Settings → Pro features**. It is off by default.
    public static let defaultsKey = "proFeatures"
    /// The UserDefaults key that remembers the user hid the start screen's hint about Pro.
    public static let hintDismissedKey = "proHintDismissed"

    public static var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: defaultsKey)
    }
}
