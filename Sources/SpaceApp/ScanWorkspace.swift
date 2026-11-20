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
    case donut = "Donut"
}

/// A sortable table column. `.reverse` means largest first for sizes and counts, Z–A for names.
struct EntrySort: SortComparator, Hashable {
    var key: EntryOrder
    var order: SortOrder

    static let largestFirst = EntrySort(key: .size, order: .reverse)

    /// The workspace sorts rows itself with the current size metric. This serves other callers.
    func compare(_ lhs: ScanEntry, _ rhs: ScanEntry) -> ComparisonResult {
        guard lhs.path != rhs.path else { return .orderedSame }
        let first = [lhs, rhs].ordered(by: key, ascending: order == .forward, metric: .allocated)[0]
        return first.path == lhs.path ? .orderedAscending : .orderedDescending
    }
}

extension SizeMetric {
    /// The word for this size in labels, matching the CLI's allocated and apparent sizes.
    var title: String {
        switch self {
        case .allocated: "Allocated"
        case .logical: "Apparent"
        }
    }
}

/// Exclude patterns from Settings, one per line, applied when a scan starts.
enum ExcludeSettings {
    static let key = "excludePatterns"

    static var patterns: [String] {
        ExcludeRules.patterns(fromLines: UserDefaults.standard.string(forKey: key) ?? "")
    }
}

@MainActor
@Observable
final class ScanWorkspace {
    nonisolated static let largestFileLimit = 100

    private(set) var report: ScanReport?
    private(set) var issuePaths: Set<String> = []
    /// Paths the OS refused to let the scan read. They are a subset of `issuePaths`.
    private(set) var protectedPaths: Set<String> = []
    private(set) var errorIssueCount = 0
    private(set) var protectedIssueCount = 0
    private(set) var hasFullDiskAccess = FullDiskAccess.isGranted
    private var scanHadFullDiskAccess = false
    private var hasPrivacyRefusals = false
    private(set) var scope: [ScanEntry] = []
    private(set) var rows: [ScanEntry] = []
    private(set) var rankedScope: ScanEntry?
    private(set) var isLoadingRows = false
    private(set) var scanPath: String?
    private(set) var scanHasResults = false
    private(set) var progress: ScanProgress?
    private(set) var isCancelling = false
    private var isPresentingPicker = false
    private(set) var notice: Notice?
    private(set) var selectedEntry: ScanEntry?
    var showsDetails = false
    var folderPresentation: FolderPresentation = .list
    /// Up to 32 entries of the current folder, largest first, for the treemap and donut.
    private(set) var chartRows: [ScanEntry] = []

    var sizeMetric: SizeMetric = SizeMetric(rawValue: UserDefaults.standard.string(forKey: "sizeMetric") ?? "")
        ?? .allocated
    {
        didSet {
            guard sizeMetric != oldValue else { return }
            UserDefaults.standard.set(sizeMetric.rawValue, forKey: "sizeMetric")
            loadRows(keepingRows: true)
        }
    }

    var sortOrder: [EntrySort] = [.largestFirst] {
        didSet {
            guard sortOrder != oldValue else { return }
            loadRows(keepingRows: true)
        }
    }

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

    @ObservationIgnored private var scanTask: Task<Void, Never>?
    @ObservationIgnored private var rowsTask: Task<Void, Never>?

    var currentFolder: ScanEntry? { scope.last }
    var visibleScope: ScanEntry? { mode == .folders ? currentFolder : report?.root }
    var isScanning: Bool { scanPath != nil }
    var canChooseFolder: Bool { !isScanning && !isPresentingPicker }
    var canBrowse: Bool { report != nil && (!isScanning || scanHasResults) }
    var canRescan: Bool { report != nil && canChooseFolder }

    /// Suggests Full Disk Access only when macOS privacy controls kept items out of this scan.
    var accessAdvice: AccessAdvice {
        guard hasPrivacyRefusals else { return .none }
        if !hasFullDiskAccess { return .grant }
        return scanHadFullDiskAccess ? .none : .rescan
    }

    func refreshFullDiskAccess() {
        let granted = FullDiskAccess.isGranted
        if granted != hasFullDiskAccess { hasFullDiskAccess = granted }
    }
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
        if let root = report?.root, !root.isCloudOnly, !issuePaths.contains(root.path) {
            picker.directoryURL = URL(filePath: root.path, directoryHint: .isDirectory)
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
        let excludes: ExcludeRules
        do {
            excludes = try ExcludeRules(patterns: ExcludeSettings.patterns)
        } catch {
            notice = Notice(
                title: "Check your exclude patterns",
                message: (error.localizedDescription) + " Fix it in Settings, then scan again.",
                isError: true
            )
            return
        }
        let didAccess = url.startAccessingSecurityScopedResource()
        let path = url.path(percentEncoded: false)
        scanPath = path
        scanHasResults = false
        refreshFullDiskAccess()
        scanHadFullDiskAccess = hasFullDiskAccess
        progress = nil
        notice = nil
        showsDetails = false

        let (updates, continuation) = AsyncStream.makeStream(
            of: ScanProgress.self,
            bufferingPolicy: .bufferingNewest(1)
        )
        let (snapshots, snapshotContinuation) = AsyncStream.makeStream(
            of: ScanReport.self,
            bufferingPolicy: .bufferingNewest(1)
        )
        let worker = Task.detached(priority: .userInitiated) {
            defer {
                continuation.finish()
                snapshotContinuation.finish()
                if didAccess { url.stopAccessingSecurityScopedResource() }
            }
            return try DiskScanner.scan(
                path: path,
                excludes: excludes,
                progress: { continuation.yield($0) },
                preview: { snapshotContinuation.yield($0) },
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
            let snapshotTask = Task { [weak self] in
                for await snapshot in snapshots {
                    guard !Task.isCancelled, let self, !self.isCancelling else { return }
                    self.acceptReport(snapshot)
                }
            }
            defer {
                progressTask.cancel()
                snapshotTask.cancel()
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
                snapshotTask.cancel()
                self?.acceptReport(result)
            } catch {
                snapshotTask.cancel()
                let keptPreview = self?.scanHasResults == true
                let previewMessage = "Showing the allocated space counted so far."
                if error is CancellationError || Task.isCancelled {
                    self?.notice = Notice(title: "Scan cancelled", message: keptPreview ? previewMessage : path)
                } else {
                    self?.notice = Notice(
                        title: keptPreview ? "Scan stopped before it finished" : "Couldn’t scan this location",
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
        guard canBrowse, entry.isDirectory, mode == .folders,
              let current = currentFolder?.children.first(where: { $0.id == entry.id && $0.isDirectory })
        else { return }
        scope.append(current)
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

    private func acceptReport(_ result: ScanReport) {
        let continuing = scanHasResults
        scanHasResults = true
        report = result
        issuePaths = Set(result.issues.map(\.path))
        let protectedIssues = result.protectedIssues
        protectedPaths = Set(protectedIssues.map(\.path))
        protectedIssueCount = protectedIssues.count
        errorIssueCount = result.issues.count - protectedIssues.count
        hasPrivacyRefusals = protectedIssues.contains { $0.kind == .notPermitted }
        let oldScope = scope
        scope = [result.root]
        if continuing {
            for folder in oldScope.dropFirst() {
                guard let next = scope.last?.children.first(where: { $0.id == folder.id && $0.isDirectory }) else { break }
                scope.append(next)
            }
            let sameLocation = mode == .largestFiles || oldScope.last?.id == scope.last?.id
            if !sameLocation { selection = nil }
            loadRows(keepingRows: sameLocation)
        } else if mode != .folders {
            mode = .folders
        } else {
            selection = nil
            loadRows()
        }
    }

    private func loadRows(keepingRows: Bool = false) {
        let previousTask = rowsTask
        previousTask?.cancel()
        if !keepingRows {
            rows = []
            chartRows = []
            rankedScope = nil
            selectedEntry = nil
        }
        guard let report, let folder = currentFolder else {
            isLoadingRows = false
            return
        }
        isLoadingRows = true
        let mode = mode
        let metric = sizeMetric
        let sort = sortOrder.first ?? .largestFirst
        rowsTask = Task { [weak self] in
            // Let a running sort release its buffers before allocating for a newer ranking.
            await previousTask?.value
            guard !Task.isCancelled else { return }
            let worker = Task.detached(priority: .userInitiated) {
                let ranked = switch mode {
                case .folders: folder.rankedChildren(by: metric)
                case .largestFiles: report.largestFiles(limit: Self.largestFileLimit, by: metric)
                }
                let rows = sort == .largestFirst
                    ? ranked
                    : ranked.ordered(by: sort.key, ascending: sort.order == .forward, metric: metric)
                return (rows: rows, chart: Array(ranked.prefix(32)))
            }
            let result = await withTaskCancellationHandler {
                await worker.value
            } onCancel: {
                worker.cancel()
            }
            guard !Task.isCancelled, let self else { return }
            rankedScope = mode == .folders ? folder : report.root
            rows = result.rows
            chartRows = result.chart
            if let selection, entry(withPath: selection) == nil { self.selection = nil }
            selectedEntry = selection.flatMap(entry(withPath:))
            isLoadingRows = false
            rowsTask = nil
        }
    }
}
