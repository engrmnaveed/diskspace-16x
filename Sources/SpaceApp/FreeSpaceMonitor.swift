import AppKit
import SpaceCore
import SpacePro
import SwiftUI
import UserNotifications

/// Pro: the startup disk's free space in the menu bar, refreshed once a minute, with one
/// notification when it drops below the threshold in Settings. It runs only while
/// **Settings → Pro features → Show free space in the menu bar** is on.
@MainActor
@Observable
final class FreeSpaceMonitor {
    private(set) var volumes: [VolumeSummary] = []
    @ObservationIgnored private var alert = LowSpaceAlert()
    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private var isRefreshing = false
    @ObservationIgnored private var settingsObserver: NSObjectProtocol?

    init() {
        // Settings write UserDefaults, so follow them without a view's help.
        settingsObserver = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.followSettings() }
        }
        followSettings()
    }

    static var isEnabled: Bool {
        ProFeatures.isEnabled && UserDefaults.standard.bool(forKey: LowSpaceAlert.monitorKey)
    }

    var startupVolume: VolumeSummary? { volumes.first { $0.path == "/" } ?? volumes.first }

    /// The menu bar's text: the startup disk's free space.
    var title: String { startupVolume.map { ScanFormat.size($0.availableBytes) } ?? "–" }

    private func followSettings() {
        if Self.isEnabled { start() } else { stop() }
    }

    private func start() {
        guard timer == nil else { return }
        refresh()
        let timer = Timer(timeInterval: 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        timer.tolerance = 15
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        if canNotify {
            Task { _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) }
        }
    }

    private func stop() {
        timer?.invalidate()
        timer = nil
        alert = LowSpaceAlert()
    }

    func refresh() {
        guard !isRefreshing else { return }
        isRefreshing = true
        Task {
            let mounted = await Task.detached(priority: .utility) { VolumeSummary.mounted() }.value
            isRefreshing = false
            guard timer != nil else { return }
            if mounted != volumes { volumes = mounted }
            checkFreeSpace()
        }
    }

    private func checkFreeSpace() {
        guard let volume = startupVolume else { return }
        let threshold = UserDefaults.standard.object(forKey: LowSpaceAlert.thresholdKey) as? Int
            ?? LowSpaceAlert.defaultThresholdGB
        guard alert.update(freeBytes: volume.availableBytes, thresholdGB: threshold), canNotify else { return }
        let content = UNMutableNotificationContent()
        content.title = "\(volume.name) is almost full"
        content.body = "\(ScanFormat.size(volume.availableBytes)) free. Open DiskUse16x to see what's using the space."
        UNUserNotificationCenter.current().add(
            UNNotificationRequest(identifier: "low-space", content: content, trigger: nil)
        )
    }

    /// Notifications need an app bundle, which `swift run` doesn't have.
    private var canNotify: Bool { Bundle.main.bundleIdentifier != nil }
}

/// The menu-bar item's menu: each volume's free space and a way back to the app.
struct FreeSpaceMenu: View {
    let monitor: FreeSpaceMonitor
    @Environment(\.openWindow) private var openWindow
    @AppStorage(LowSpaceAlert.monitorKey) private var showsMonitor = false

    var body: some View {
        ForEach(monitor.volumes) { volume in
            Text("\(volume.name): \(ScanFormat.size(volume.availableBytes)) free of \(ScanFormat.size(volume.totalBytes))")
        }
        Divider()
        Button("Open DiskUse16x") {
            NSApp.activate()
            // Bring back a window that's open, or open one when all are closed.
            let windows = NSApp.windows.filter { $0.identifier?.rawValue.hasPrefix("workspace") == true && $0.isVisible }
            if let window = windows.first {
                window.makeKeyAndOrderFront(nil)
            } else {
                openWindow(id: "workspace")
            }
        }
        SettingsLink { Text("Settings…") }
        Divider()
        Button("Hide from Menu Bar") { showsMonitor = false }
    }
}
