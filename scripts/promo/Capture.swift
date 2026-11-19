import AppKit
import SwiftUI

// Build alongside the shipping views without their application entry point.
typealias ViewState<Value> = SwiftUI.State<Value>

@main
struct Capture {
    @MainActor
    static func main() async {
        do {
            try await render()
        } catch {
            FileHandle.standardError.write(Data("Promo capture: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }

    @MainActor
    private static func render() async throws {
        let arguments = CommandLine.arguments
        guard arguments.count == 6 else {
            throw CaptureError("Expected the demo folder, working directory, CLI report, explorer capture, and app icon.")
        }
        let fixture = URL(filePath: arguments[1], directoryHint: .isDirectory)
        let output = URL(filePath: arguments[2], directoryHint: .isDirectory)
        let reportLines = try String(contentsOfFile: arguments[3], encoding: .utf8)
            .split(separator: "\n").map(String.init)
        let explorer = try JSONDecoder().decode(ExplorerCapture.self, from: Data(contentsOf: URL(filePath: arguments[4])))
        var iconRect = CGRect(x: 0, y: 0, width: 1024, height: 1024)
        guard let icon = NSImage(contentsOfFile: arguments[5])?.cgImage(forProposedRect: &iconRect,
                                                                        context: nil, hints: nil) else {
            throw CaptureError("Could not load the Space 16X icon.")
        }
        NSApplication.shared.setActivationPolicy(.prohibited)

        let workspace = ScanWorkspace()
        let size = CGSize(width: 810, height: 510)
        let host = NSHostingView(rootView: WorkspaceView(workspace: workspace).preferredColorScheme(.dark))
        host.frame = CGRect(origin: .zero, size: size)
        let window = NSWindow(
            contentRect: host.frame, styleMask: [.titled, .closable, .resizable],
            backing: .buffered, defer: false
        )
        window.title = "Space 16X"
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = host

        workspace.startScan(fixture)
        try await waitUntil { !workspace.isScanning && !workspace.isLoadingRows }
        guard let report = workspace.report, !report.isPartial else {
            throw CaptureError("The demo scan must complete without partial results.")
        }

        var desktop: [String: CGImage] = [:]
        for (name, mode, presentation) in [
            ("treemap", BrowseMode.folders, FolderPresentation.treemap),
            ("donut", .folders, .donut),
            ("files", .largestFiles, .list),
        ] {
            workspace.mode = mode
            workspace.folderPresentation = presentation
            try await waitUntil { !workspace.isLoadingRows }
            try await Task.sleep(for: .milliseconds(350))
            host.layoutSubtreeIfNeeded()
            guard let bitmap = host.bitmapImageRepForCachingDisplay(in: host.bounds) else {
                throw CaptureError("Could not allocate the \(name) snapshot.")
            }
            host.cacheDisplay(in: host.bounds, to: bitmap)
            guard let image = bitmap.cgImage else {
                throw CaptureError("Could not render the \(name) snapshot.")
            }
            desktop[name] = image
        }
        try Cards(output: output, icon: icon, report: reportLines, explorer: explorer, desktop: desktop).render()
    }

    @MainActor
    private static func waitUntil(_ predicate: () -> Bool) async throws {
        for _ in 0..<200 {
            if predicate() { return }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw CaptureError("Timed out waiting for the demo scan or ranking.")
    }
}

private struct CaptureError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}
