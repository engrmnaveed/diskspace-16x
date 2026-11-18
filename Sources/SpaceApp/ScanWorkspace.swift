import AppKit
import Observation
import SpaceCore

enum BrowseMode: String, CaseIterable {
    case folders = "Folders"
    case largestFiles = "Largest Files"

    var symbol: String {
        switch self {
        case .folders: "folder"
        case .largestFiles: "list.number"
        }
    }
}

enum FolderPresentation: String, CaseIterable {
    case list = "List"
    case treemap = "Treemap"
}

@MainActor
@Observable
final class ScanWorkspace {
    nonisolated static let largestFileLimit = 100

    private(set) var report: ScanReport?
    private(set) var scope: [ScanEntry] = []
    private(set) var rows: [ScanEntry] = []
    private(set) var isLoadingRows = false
    private(set) var scanPath: String?
    private(set) var progress: ScanProgress?
    private(set) var isCancelling = false
    private var isPresentingPicker = false
    private(set) var notice: Notice?
    private(set) var selectedEntry: ScanEntry?
    var showsDetails = false
    var folderPresentation: FolderPresentation = .list

    var selection: String? {
        didSet {
            selectedEntry = selection.flatMap(entry(withPath:))
        }
    }

    var mode: BrowseMode = .folders {
        didSet {
            guard mode != oldValue else { return }
            selection = nil
            loadRows()
        }
    }

    var metric: SizeMetric = .allocated {
        didSet {
            if metric != oldValue { loadRows() }
        }
    }

    @ObservationIgnored private var scanTask: Task<Void, Never>?
    @ObservationIgnored private var rowsTask: Task<Void, Never>?

    var currentFolder: ScanEntry? { scope.last }
    var visibleScope: ScanEntry? { mode == .folders ? currentFolder : report?.root }
    var isScanning: Bool { scanPath != nil }
    var canChooseFolder: Bool { !isScanning && !isPresentingPicker }
    var canBrowse: Bool { report != nil && !isScanning }
    var canRescan: Bool { report != nil && canChooseFolder }
    var canBrowseParent: Bool { canBrowse && mode == .folders && scope.count > 1 }

    struct Notice {
        let title: String
        let message: String
        var isError = false
    }

    func chooseFolder() {
        guard canChooseFolder else { return }
        let picker = NSOpenPanel()
        picker.title = "Choose a Folder to Scan"
        picker.prompt = "Scan"
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.allowsMultipleSelection = false
        picker.treatsFilePackagesAsDirectories = true
        picker.resolvesAliases = false
        if let path = report?.root.path {
            picker.directoryURL = URL(filePath: path, directoryHint: .isDirectory)
        }

        isPresentingPicker = true
        Task {
            let result = await picker.begin()
            isPresentingPicker = false
            if result == .OK, let url = picker.url { startScan(url) }
        }
    }

    @discardableResult
    func acceptDrop(_ urls: [URL]) -> Bool {
        guard canChooseFolder, !urls.isEmpty else { return false }
        guard urls.count == 1, let url = urls.first else {
            notice = Notice(
                title: "Choose one folder or drive",
                message: "Space 16X scans one location at a time.",
                isError: true
            )
            return false
        }
        guard url.isFileURL else {
            notice = Notice(
                title: "Choose a local folder or drive",
                message: "Use the folder picker or drag the location from Finder.",
                isError: true
            )
            return false
        }
        startScan(url)
        return true
    }

    func rescan() {
        guard let path = report?.root.path else { return }
        startScan(URL(filePath: path, directoryHint: .isDirectory))
    }

    func startScan(_ url: URL) {
        guard canChooseFolder else { return }
        let didAccess = url.startAccessingSecurityScopedResource()
        let path = url.path(percentEncoded: false)
        scanPath = path
        progress = nil
        notice = nil
        showsDetails = false

        let (updates, continuation) = AsyncStream.makeStream(
            of: ScanProgress.self,
            bufferingPolicy: .bufferingNewest(1)
        )
        let worker = Task.detached(priority: .userInitiated) {
            defer {
                continuation.finish()
                if didAccess { url.stopAccessingSecurityScopedResource() }
            }
            return try DiskScanner.scan(
                path: path,
                progress: { continuation.yield($0) },
                isCancelled: { Task.isCancelled }
            )
        }

        scanTask = Task { [weak self] in
            let progressTask = Task { [weak self] in
                for await update in updates {
                    guard !Task.isCancelled else { return }
                    self?.progress = update
                    try? await Task.sleep(for: .milliseconds(100))
                }
            }
            defer {
                progressTask.cancel()
                self?.scanTask = nil
                self?.scanPath = nil
                self?.progress = nil
                self?.isCancelling = false
            }
            do {
                let result = try await withTaskCancellationHandler {
                    try await worker.value
                } onCancel: {
                    worker.cancel()
                }
                try Task.checkCancellation()
                self?.useReport(result)
            } catch {
                if error is CancellationError || Task.isCancelled {
                    self?.notice = Notice(title: "Scan cancelled", message: path)
                } else {
                    self?.notice = Notice(
                        title: "Couldn’t scan this location",
                        message: error.localizedDescription,
                        isError: true
                    )
                }
            }
        }
    }

    func cancelScan() {
        guard isScanning, !isCancelling else { return }
        isCancelling = true
        scanTask?.cancel()
    }

    func dismissNotice() { notice = nil }

    func browse(_ entry: ScanEntry) {
        guard canBrowse, entry.isDirectory, mode == .folders else { return }
        scope.append(entry)
        selection = nil
        loadRows()
    }

    func browseBreadcrumb(at index: Int) {
        guard canBrowse, scope.indices.contains(index), index < scope.count - 1 else { return }
        let childPath = scope[index + 1].path
        scope.removeSubrange((index + 1)...)
        rows = []
        selection = childPath
        loadRows()
    }

    func browseParent() {
        guard canBrowseParent else { return }
        browseBreadcrumb(at: scope.count - 2)
    }

    func entry(withPath path: String) -> ScanEntry? { rows.first { $0.path == path } }

    func openSelectedFolder() {
        if let entry = selectedEntry { browse(entry) }
    }

    func activate(_ entry: ScanEntry) {
        if entry.isDirectory {
            browse(entry)
        } else {
            reveal(entry)
        }
    }

    func revealSelected() {
        if let entry = selectedEntry { reveal(entry) }
    }

    func copySelectedPath() {
        if let entry = selectedEntry { copyPath(of: entry) }
    }

    func reveal(_ entry: ScanEntry) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: entry.path)])
    }

    func copyPath(of entry: ScanEntry) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(entry.path, forType: .string)
    }

    private func useReport(_ result: ScanReport) {
        report = result
        scope = [result.root]
        rows = []
        selection = nil
        if mode == .folders {
            loadRows()
        } else {
            mode = .folders
        }
    }

    private func loadRows() {
        let previousTask = rowsTask
        previousTask?.cancel()
        rows = []
        selectedEntry = nil
        guard let report, let folder = currentFolder else {
            isLoadingRows = false
            return
        }
        isLoadingRows = true
        let mode = mode
        let metric = metric
        rowsTask = Task { [weak self] in
            // Let a running sort release its buffers before allocating for a newer ranking.
            await previousTask?.value
            guard !Task.isCancelled else { return }
            let worker = Task.detached(priority: .userInitiated) {
                switch mode {
                case .folders:
                    folder.rankedChildren(by: metric)
                case .largestFiles:
                    report.largestFiles(limit: Self.largestFileLimit, by: metric)
                }
            }
            let result = await withTaskCancellationHandler {
                await worker.value
            } onCancel: {
                worker.cancel()
            }
            guard !Task.isCancelled, let self else { return }
            rows = result
            selectedEntry = selection.flatMap(entry(withPath:))
            isLoadingRows = false
            rowsTask = nil
        }
    }
}
