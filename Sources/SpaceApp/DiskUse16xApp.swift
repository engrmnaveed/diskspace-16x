import AppKit
import SpaceCore
import SpacePro
import SwiftUI

// Some Command Line Tools SDKs expose the State macro without its compiler plugin.
typealias ViewState<Value> = SwiftUI.State<Value>

@main
struct DiskUse16xApp: App {
    @ViewState private var freeSpace = FreeSpaceMonitor()
    @AppStorage(ProFeatures.defaultsKey) private var proFeatures = false
    @AppStorage(LowSpaceAlert.monitorKey) private var showsFreeSpace = false

    init() {
        // New windows come from File → New Window in Pro, not from tabs.
        NSWindow.allowsAutomaticWindowTabbing = false
        // Space 16X, the name before 1.0, kept its settings under another bundle ID.
        LegacySettings.importOnce(
            from: UserDefaults.standard.persistentDomain(forName: LegacySettings.domain),
            into: UserDefaults.standard
        )
    }

    var body: some Scene {
        WindowGroup("DiskUse16x", id: "workspace") {
            WorkspaceWindow()
        }
        .defaultSize(width: 1040, height: 700)
        .windowResizability(.contentMinSize)
        .commands { WorkspaceCommands() }

        Settings {
            SettingsView()
        }

        // Pro: free space in the menu bar, only while Settings turns it on.
        MenuBarExtra(isInserted: Binding(
            get: { proFeatures && showsFreeSpace },
            set: { showsFreeSpace = $0 }
        )) {
            FreeSpaceMenu(monitor: freeSpace)
        } label: {
            Label(freeSpace.title, systemImage: "internaldrive")
                .labelStyle(.titleAndIcon)
                .accessibilityLabel("\(freeSpace.title) free on the startup disk")
        }
    }
}
