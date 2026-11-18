import AppKit
import SwiftUI

// Some Command Line Tools SDKs expose the State macro without its compiler plugin.
typealias ViewState<Value> = SwiftUI.State<Value>

@main
struct Space16XApp: App {
    @ViewState private var workspace = ScanWorkspace()

    var body: some Scene {
        Window("Space 16X", id: "workspace") {
            WorkspaceView(workspace: workspace)
                .frame(minWidth: 760, minHeight: 500)
                .onDisappear { workspace.cancelScan() }
        }
        .defaultSize(width: 1040, height: 700)
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Choose Folder…", action: workspace.chooseFolder)
                    .keyboardShortcut("o")
                    .disabled(!workspace.canChooseFolder)
            }
            CommandMenu("Scan") {
                Button("Scan Again", action: workspace.rescan)
                    .keyboardShortcut("r")
                    .disabled(!workspace.canRescan)
                Button("Cancel Scan", action: workspace.cancelScan)
                    .keyboardShortcut(".", modifiers: .command)
                    .disabled(!workspace.isScanning || workspace.isCancelling)
                Divider()
                Button("Show Scan Details") { workspace.showsDetails = true }
                    .keyboardShortcut("i")
                    .disabled(!workspace.canBrowse)
            }
            CommandMenu("Browse") {
                Button("Folder List") {
                    workspace.mode = .folders
                    workspace.folderPresentation = .list
                }
                .keyboardShortcut("1")
                .disabled(!workspace.canBrowse)
                Button("Largest Files") { workspace.mode = .largestFiles }
                    .keyboardShortcut("2")
                    .disabled(!workspace.canBrowse)
                Button("Folder Treemap") {
                    workspace.mode = .folders
                    workspace.folderPresentation = .treemap
                }
                .keyboardShortcut("3")
                .disabled(!workspace.canBrowse)
                Button("Folder Donut") {
                    workspace.mode = .folders
                    workspace.folderPresentation = .donut
                }
                .keyboardShortcut("4")
                .disabled(!workspace.canBrowse)
                Divider()
                Button("Open Selected Folder", action: workspace.openSelectedFolder)
                    .keyboardShortcut(.downArrow, modifiers: .command)
                    .disabled(workspace.selectedEntry?.isDirectory != true || !workspace.canBrowse)
                Button("Go to Parent Folder", action: workspace.browseParent)
                    .keyboardShortcut(.upArrow, modifiers: .command)
                    .disabled(!workspace.canBrowseParent)
                Divider()
                Button("Reveal in Finder", action: workspace.revealSelected)
                    .keyboardShortcut("r", modifiers: [.command, .shift])
                    .disabled(workspace.selectedEntry == nil || !workspace.canBrowse)
                Button("Copy Path", action: workspace.copySelectedPath)
                    .keyboardShortcut("c", modifiers: [.command, .option])
                    .disabled(workspace.selectedEntry == nil || !workspace.canBrowse)
            }
        }
    }
}
