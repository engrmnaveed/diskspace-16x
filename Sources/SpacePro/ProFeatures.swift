import Foundation

/// Space 16X Pro in the Mac app. Pro analysis lives in this target, which depends on
/// SpaceCore, and the app shows its features only after the user turns them on.
public enum ProFeatures {
    /// The UserDefaults key behind **Settings → Pro features**. It is off by default.
    public static let defaultsKey = "proFeatures"

    public static var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: defaultsKey)
    }
}
