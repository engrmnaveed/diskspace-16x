import AppKit
import SpaceCore
import SpacePro
import SwiftUI

// Build alongside the shipping views without their application entry point.
typealias ViewState<Value> = SwiftUI.State<Value>

/// Reports the app as active, so the offscreen windows draw their controls as a focused
/// window does. The captures never show a window or take the focus.
final class ActiveApplication: NSApplication {
    override var isActive: Bool { true }
}

final class KeyWindow: NSWindow {
    override var isKeyWindow: Bool { true }
    override var isMainWindow: Bool { true }
}

struct GuideError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

/// Renders the user guide's screenshots, GIF frames and video scenes from the shipping
/// SwiftUI views and the terminal frames that `scripts/guide/terminal.py` captured.
@main
struct GuideCapture {
    @MainActor
    static func main() async {
        do {
            _ = ActiveApplication.shared
            NSApp.setActivationPolicy(.prohibited)
            let arguments = CommandLine.arguments
            guard arguments.count == 5 else {
                throw GuideError("Expected the home folder, the baseline scan, the specification and the app icon.")
            }
            let spec = try JSONDecoder().decode(Spec.self, from: Data(contentsOf: URL(filePath: arguments[3])))
            var iconRect = CGRect(x: 0, y: 0, width: 1024, height: 1024)
            guard let icon = NSImage(contentsOfFile: arguments[4]),
                  let iconImage = icon.cgImage(forProposedRect: &iconRect, context: nil, hints: nil)
            else { throw GuideError("Could not load the DiskUse16x icon.") }
            let capture = MacCapture(
                home: URL(filePath: arguments[1], directoryHint: .isDirectory),
                baseline: URL(filePath: arguments[2]), icon: icon
            )
            let windows = try await capture.run()
            try Compositor(spec: spec, windows: windows, icon: iconImage).render()
        } catch {
            FileHandle.standardError.write(Data("Guide capture: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }
}

/// Drives one workspace through the states the guide shows and keeps each as an image of
/// the window, with its title bar drawn in. Registered defaults stay in memory, so the
/// capture leaves no preferences behind.
@MainActor
final class MacCapture {
    static let size = CGSize(width: 960, height: 640)

    let home: URL
    let baseline: URL
    let icon: NSImage
    let workspace = ScanWorkspace()
    let host: NSView
    let window: KeyWindow
    var windows: [String: CGImage] = [:]

    init(home: URL, baseline: URL, icon: NSImage) {
        self.home = home
        self.baseline = baseline
        self.icon = icon
        let root = WorkspaceView(workspace: workspace)
            .preferredColorScheme(.light)
            .environment(\.controlActiveState, .key)
        host = NSHostingView(rootView: root)
        host.frame = CGRect(origin: .zero, size: Self.size)
        window = KeyWindow(
            contentRect: host.frame, styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false
        )
        window.appearance = NSAppearance(named: .aqua)
        window.contentView = host
    }

    func run() async throws -> [String: CGImage] {
        UserDefaults.standard.register(defaults: [ProFeatures.defaultsKey: false])

        // The start screen lists made-up volumes, not this Mac's disks.
        try await waitUntil { !self.workspace.volumes.isEmpty }
        workspace.volumes = [
            VolumeSummary(name: "Macintosh HD", path: "/", totalBytes: 994_662_584_320, availableBytes: 214_380_000_000),
            VolumeSummary(name: "Backup Drive", path: "/Volumes/Backup Drive",
                          totalBytes: 2_000_000_000_000, availableBytes: 1_240_000_000_000),
        ]
        try await keep("start")

        workspace.startScan(home)
        try await waitUntil { !self.workspace.isScanning && !self.workspace.isLoadingRows }
        guard let report = workspace.report, !report.isPartial else {
            throw GuideError("The guide's folder must scan completely.")
        }
        try await keep("folders")

        workspace.browse(try entry(named: "Movies"))
        try await keep("movies")
        workspace.browseParent()

        workspace.folderPresentation = .treemap
        try await keep("treemap")
        workspace.folderPresentation = .donut
        try await keep("donut")
        workspace.folderPresentation = .list

        workspace.mode = .largestFiles
        try await keep("largest")
        workspace.selectedPaths = [try entry(named: "Wedding edit (1).mp4").path, try entry(named: "Installer.dmg").path]
        try await keep("largest-selected")
        workspace.requestTrash()
        try await waitUntil { !self.workspace.pendingTrash.isEmpty }
        windows["largest-alert"] = try sheet(trashAlert(), over: windows["largest-selected"]!)
        workspace.pendingTrash = []
        workspace.selectedPaths = []
        workspace.mode = .folders

        windows["details"] = try sheet(
            render(ScanDetailsView(report: report, workspace: workspace)), over: windows["folders"]!
        )

        // Pro, as Settings → Pro features turns it on.
        UserDefaults.standard.register(defaults: [ProFeatures.defaultsKey: true, LowSpaceAlert.monitorKey: true])
        try await Task.sleep(for: .milliseconds(400))
        try await keep("pro-folders")
        workspace.folderPresentation = .types
        try await keep("pro-types")
        workspace.folderPresentation = .list
        workspace.olderThanDays = 365
        try await keep("pro-older")
        workspace.olderThanDays = nil
        workspace.showsExclusive = true
        try await waitUntil { self.workspace.exclusiveSizes != nil && self.workspace.exclusiveProgress == nil }
        try await keep("pro-exclusive")
        workspace.showsExclusive = false

        workspace.mode = .cleanup
        try await keep("pro-cleanup")
        workspace.mode = .duplicates
        try await keep("pro-duplicates-start")
        workspace.findDuplicates()
        try await waitUntil { self.workspace.duplicateGroups != nil && !self.workspace.isFindingDuplicates }
        workspace.dismissNotice()
        try await keep("pro-duplicates")

        workspace.mode = .folders
        workspace.compare(with: baseline, name: "Last week", date: Date(timeIntervalSinceNow: -7 * 86_400))
        try await waitUntil { self.workspace.comparison != nil }
        workspace.sortOrder = [.biggestGrowth]
        try await keep("pro-compare")
        windows["pro-receipt"] = try sheet(render(ReceiptSheet(workspace: workspace)), over: windows["pro-compare"]!)
        workspace.stopComparing()
        workspace.sortOrder = [.largestFirst]
        workspace.dismissNotice()
        // The explanation is taller than the window, so this one capture uses a taller window.
        let explanation = try render(ExplainDiskView(explanation: Self.explanation))
        let tall = CGSize(width: Self.size.width,
                          height: max(Self.size.height, CGFloat(explanation.height) / window.backingScaleFactor + 96))
        host.frame.size = tall
        window.setContentSize(tall)
        try await settle()
        windows["pro-explain"] = try sheet(explanation, over: Chrome.app(snapshot(host), scale: window.backingScaleFactor))
        host.frame.size = Self.size
        window.setContentSize(Self.size)

        // Settings last, since its exclude patterns would apply to a new scan.
        UserDefaults.standard.register(defaults: [ExcludeSettings.key: "node_modules\n*.log\n"])
        windows["settings"] = try settingsWindow()
        return windows
    }

    /// A made-up startup disk, so the guide shows no real volume.
    static let explanation = DiskExplanation(
        mountPoint: "/System/Volumes/Data", name: "Macintosh HD", fileSystem: "apfs", device: "/dev/disk3s5",
        totalBytes: 994_662_584_320, usedBytes: 780_282_584_320, availableBytes: 214_380_000_000,
        purgeableBytes: 12_410_000_000,
        snapshots: ["com.apple.TimeMachine.2026-11-21-093012.local", "com.apple.TimeMachine.2026-11-22-083045.local"],
        containerVolumes: [
            .init(path: "/System/Volumes/Data", usedBytes: 746_830_000_000),
            .init(path: "/", usedBytes: 13_760_000_000),
            .init(path: "/System/Volumes/Preboot", usedBytes: 10_810_000_000),
            .init(path: "/System/Volumes/VM", usedBytes: 6_440_000_000),
            .init(path: "/Volumes/Recovery", usedBytes: 2_560_000_000),
        ]
    )

    private func entry(named name: String) throws -> ScanEntry {
        guard let entry = workspace.rows.first(where: { $0.name == name }) else {
            throw GuideError("The guide's folder has no “\(name)” in view.")
        }
        return entry
    }

    private func settle() async throws {
        try await waitUntil { !self.workspace.isLoadingRows }
        try await Task.sleep(for: .milliseconds(400))
    }

    private func keep(_ name: String) async throws {
        try await settle()
        windows[name] = try Chrome.app(snapshot(host), scale: window.backingScaleFactor)
    }

    /// The Move to Trash confirmation, with the text that `ResultsView` shows in its alert.
    private func trashAlert() throws -> CGImage {
        let items = workspace.pendingTrash
        let bytes = items.reduce(UInt64(0)) { $0 + $1.allocatedBytes }
        let alert = NSAlert()
        alert.messageText = items.count == 1 ? "Move “\(items[0].name)” to the Trash?" : "Move \(items.count) items to the Trash?"
        alert.informativeText = "\(ScanFormat.size(bytes)) of allocated space. Space is freed when you empty the Trash."
        alert.icon = icon
        alert.addButton(withTitle: "Move to Trash").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        alert.layout()
        alert.window.appearance = NSAppearance(named: .aqua)
        guard let frame = alert.window.contentView?.superview else { throw GuideError("The alert has no frame view.") }
        frame.layoutSubtreeIfNeeded()
        return try snapshot(frame)
    }

    private func render(_ view: some View) throws -> CGImage {
        // At its ideal height, so wrapped text isn't clipped.
        let hosting = NSHostingView(rootView: view.fixedSize(horizontal: false, vertical: true)
            .preferredColorScheme(.light).environment(\.controlActiveState, .key))
        hosting.frame = CGRect(origin: .zero, size: hosting.fittingSize)
        let sheetWindow = KeyWindow(contentRect: hosting.frame, styleMask: [.titled], backing: .buffered, defer: false)
        sheetWindow.appearance = NSAppearance(named: .aqua)
        sheetWindow.contentView = hosting
        let image = try snapshot(hosting)
        let canvas = GuideCanvas(width: image.width, height: image.height)
        canvas.fill(CGRect(x: 0, y: 0, width: image.width, height: image.height), color: 0xF0F0F0)
        canvas.image(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return canvas.image()
    }

    private func settingsWindow() throws -> CGImage {
        let hosting = NSHostingView(rootView: SettingsView().preferredColorScheme(.light).environment(\.controlActiveState, .key))
        hosting.frame = CGRect(origin: .zero, size: hosting.fittingSize)
        let settings = KeyWindow(contentRect: hosting.frame, styleMask: [.titled], backing: .buffered, defer: false)
        settings.appearance = NSAppearance(named: .aqua)
        settings.contentView = hosting
        return try Chrome.plain(snapshot(hosting), title: "DiskUse16x Settings", scale: settings.backingScaleFactor)
    }

    /// A sheet over its window, as macOS shows an alert or a sheet below the title bar.
    private func sheet(_ sheet: CGImage, over base: CGImage) throws -> CGImage {
        let scale = CGFloat(base.width) / Self.size.width
        let canvas = GuideCanvas(width: base.width, height: base.height)
        canvas.image(base, in: CGRect(x: 0, y: 0, width: base.width, height: base.height))
        canvas.fill(CGRect(x: 0, y: 0, width: base.width, height: base.height), color: 0x000000, alpha: 0.10)
        let width = CGFloat(sheet.width), height = CGFloat(sheet.height)
        let rect = CGRect(x: (CGFloat(base.width) - width) / 2, y: 52 * scale, width: width, height: height)
        canvas.shadowed(rect, radius: 12 * scale, blur: 28 * scale, alpha: 0.30)
        canvas.image(sheet, in: rect, radius: 12 * scale)
        canvas.stroke(rect, color: 0x000000, alpha: 0.12, radius: 12 * scale, width: 1)
        return canvas.image()
    }

    private func snapshot(_ view: NSView) throws -> CGImage {
        view.layoutSubtreeIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
            throw GuideError("Could not allocate a snapshot.")
        }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        guard let image = bitmap.cgImage else { throw GuideError("Could not render a snapshot.") }
        return image
    }

    private func waitUntil(_ predicate: () -> Bool) async throws {
        for _ in 0..<600 {
            if predicate() { return }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw GuideError("Timed out waiting for the app to finish a step.")
    }
}

/// Window title bars. An offscreen render has no window frame, so these draw one in.
@MainActor
enum Chrome {
    static let sidebarWidth: CGFloat = 208
    static let toolbarHeight: CGFloat = 52

    /// The app window. Its content already leaves room for the toolbar at the top.
    static func app(_ content: CGImage, scale: CGFloat) -> CGImage {
        let canvas = GuideCanvas(width: content.width, height: content.height)
        let size = CGRect(x: 0, y: 0, width: content.width, height: content.height)
        canvas.fill(size, color: 0xFFFFFF)
        canvas.image(content, in: size)
        lights(on: canvas, y: toolbarHeight / 2 * scale, scale: scale)
        canvas.symbol("sidebar.left", center: CGPoint(x: 166 * scale, y: toolbarHeight / 2 * scale),
                      size: 15 * scale, color: 0x6E7378)
        canvas.text("DiskUse16x", x: (sidebarWidth + 22) * scale, y: (toolbarHeight / 2 - 9) * scale,
                    size: 15 * scale, weight: .bold, color: 0x262A2E)
        let right = CGFloat(content.width)
        canvas.symbol("arrow.clockwise", center: CGPoint(x: right - 76 * scale, y: toolbarHeight / 2 * scale),
                      size: 15 * scale, color: 0x6E7378)
        canvas.symbol("folder.badge.plus", center: CGPoint(x: right - 34 * scale, y: toolbarHeight / 2 * scale),
                      size: 15 * scale, color: 0x6E7378)
        return canvas.image()
    }

    /// A window with a plain title bar above its content, such as Settings.
    static func plain(_ content: CGImage, title: String, scale: CGFloat) -> CGImage {
        let bar = 30 * scale
        let canvas = GuideCanvas(width: content.width, height: content.height + Int(bar))
        canvas.fill(CGRect(x: 0, y: 0, width: CGFloat(content.width), height: canvas.height), color: 0xF0F0F0)
        canvas.fill(CGRect(x: 0, y: 0, width: CGFloat(content.width), height: bar), color: 0xF3F3F3)
        canvas.image(content, in: CGRect(x: 0, y: bar, width: CGFloat(content.width), height: CGFloat(content.height)))
        canvas.fill(CGRect(x: 0, y: bar - 1, width: CGFloat(content.width), height: 1), color: 0xDDDDDD)
        lights(on: canvas, y: bar / 2, scale: scale)
        canvas.text(title, x: 0, y: bar / 2 - 8 * scale, size: 13 * scale, weight: .semibold, color: 0x3A3D40,
                    width: CGFloat(content.width), align: .center)
        return canvas.image()
    }

    private static func lights(on canvas: GuideCanvas, y: CGFloat, scale: CGFloat) {
        for (index, color) in [UInt32(0xFF5F57), 0xFEBC2E, 0x28C840].enumerated() {
            let center = CGPoint(x: (20 + CGFloat(index) * 20) * scale, y: y)
            canvas.fill(CGRect(x: center.x - 6 * scale, y: center.y - 6 * scale, width: 12 * scale, height: 12 * scale),
                        color: color, radius: 6 * scale)
        }
    }
}
