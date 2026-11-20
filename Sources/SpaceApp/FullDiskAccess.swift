import AppKit
import Darwin

/// Full Disk Access lets the app read folders that macOS privacy controls protect, such as other apps' data.
enum FullDiskAccess {
    /// Present on every supported macOS version. macOS opens it only for apps with Full Disk Access.
    private static let probePath = "/Library/Application Support/com.apple.TCC"
    private static let settingsURL = URL(
        string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
    )

    /// Opens the probe directory without listing it, so the check stays metadata-only.
    /// Only an explicit privacy refusal counts as missing access.
    static var isGranted: Bool {
        let descriptor = open(probePath, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard descriptor >= 0 else { return errno != EPERM }
        close(descriptor)
        return true
    }

    @MainActor static func openSettings() {
        guard let settingsURL else { return }
        NSWorkspace.shared.open(settingsURL)
    }
}

/// What the app can suggest about items that macOS privacy controls kept out of a scan.
enum AccessAdvice {
    /// Nothing to suggest. Any remaining protected items are readable only by macOS.
    case none
    /// Full Disk Access would let a new scan read privacy-protected folders.
    case grant
    /// Full Disk Access was turned on after this scan, so scanning again will include more.
    case rescan
}
