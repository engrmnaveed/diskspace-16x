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
    /// The folder **Rescan Folder** is scanning again, if any.
    private(set) var rescanningFolder: String?
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

    /// The selected rows. The table allows several, for Move to Trash.
    var selectedPaths: Set<String> = [] {
        didSet {
            selectedEntry = selection.flatMap(entry(withPath:))
        }
    }

    /// The one selected item, for Open, Reveal, Copy Path, and the charts. Nil when
    /// several items are selected.
    var selection: String? {
        get { selectedPaths.count == 1 ? selectedPaths.first : nil }
        set { selectedPaths = newValue.map { [$0] } ?? [] }
    }

    var selectedEntries: [ScanEntry] { rows.filter { selectedPaths.contains($0.path) } }

    /// Items waiting for the user to confirm Move to Trash. A nonempty list shows the alert.
    var pendingTrash: [ScanEntry] = []
    private(set) var isTrashing = false
    var canMoveToTrash: Bool { canBrowse && !isScanning && !isTrashing && !selectedPaths.isEmpty }

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

    /// The exclude patterns from Settings, or nil after showing why they don't parse.
    private func excludeRules() -> ExcludeRules? {
        do {
            return try ExcludeRules(patterns: ExcludeSettings.patterns)
        } catch {
            notice = Notice(
                title: "Check your exclude patterns",
                message: (error.localizedDescription) + " Fix it in Settings, then scan again.",
                isError: true
            )
            return nil
        }
    }

    /// Scans `entry`, or the open folder, again and replaces it in the results in place.
    /// In Largest Files, or at the top folder, this is a full rescan.
    func rescanFolder(_ entry: ScanEntry? = nil) {
        guard canRescan, !isTrashing, let report else { return }
        let folder = entry ?? (mode == .folders ? currentFolder : nil) ?? report.root
        guard folder.isDirectory else { return }
        guard folder.path != report.root.path else { return rescan() }
        guard let excludes = excludeRules() else { return }
        let root = report.root.path
        let path = folder.path
        scanPath = path
        rescanningFolder = folder.name
        progress = nil
        notice = nil

        let (updates, continuation) = AsyncStream.makeStream(
            of: ScanProgress.self,
            bufferingPolicy: .bufferingNewest(1)
        )
        let worker = Task.detached(priority: .userInitiated) {
            defer { continuation.finish() }
            return try DiskScanner.rescan(
                folder: path,
                in: root,
                excludes: excludes,
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
                self?.rescanningFolder = nil
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
                self?.finishRescan(of: folder, with: result)
            } catch {
                let cancelled = error is CancellationError || Task.isCancelled
                self?.failRescan(of: folder, error: error, cancelled: cancelled)
            }
        }
    }

    private func finishRescan(of folder: ScanEntry, with result: ScanReport) {
        guard let merged = report?.replacing(result) else {
            notice = Notice(
                title: "Couldn’t update “\(folder.name)”",
                message: "It’s no longer in the results. Scan again to refresh them.",
                isError: true
            )
            return
        }
        acceptReport(merged)
        let before = folder.bytes(for: sizeMetric)
        let after = result.root.bytes(for: sizeMetric)
        notice = Notice(
            title: "Rescanned “\(folder.name)”",
            message: before == after
                ? "\(ScanFormat.size(after)), unchanged."
                : "\(ScanFormat.size(after)), was \(ScanFormat.size(before))."
        )
    }

    /// A cancelled or failed rescan keeps the earlier results, except that a folder that
    /// no longer exists leaves them.
    private func failRescan(of folder: ScanEntry, error: Error, cancelled: Bool) {
        if cancelled {
            notice = Notice(title: "Rescan cancelled", message: "The earlier results stay.")
        } else if let report, (try? URL(filePath: folder.path).checkResourceIsReachable()) != true {
            acceptReport(report.removing([folder.path]))
            notice = Notice(title: "“\(folder.name)” no longer exists", message: "It left the results.")
        } else {
            notice = Notice(
                title: "Couldn’t rescan “\(folder.name)”",
                message: error.localizedDescription,
                isError: true
            )
        }
    }

    func startScan(_ url: URL) {
        guard canChooseFolder else { return }
        guard let excludes = excludeRules() else { return }
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

    /// Asks to move `entries`, or the selected items, to the Trash after checking that
    /// each one can go.
    func requestTrash(_ entries: [ScanEntry]? = nil) {
        guard let report, canBrowse, !isTrashing else { return }
        let entries = entries ?? selectedEntries
        guard !entries.isEmpty else { return }
        if isScanning {
            notice = Notice(
                title: "Let the scan finish first",
                message: "Wait for the scan to finish, or cancel it, before moving items to the Trash.",
                isError: true
            )
            return
        }
        for entry in entries {
            if let blocker = report.trashBlocker(for: entry) {
                notice = Notice(
                    title: "“\(entry.name)” stays where it is",
                    message: "It \(blocker.reason)",
                    isError: true
                )
                return
            }
        }
        pendingTrash = entries
    }

    /// Moves the confirmed items to the Trash, then removes them from the results in place.
    func confirmTrash() {
        let entries = pendingTrash
        pendingTrash = []
        guard !entries.isEmpty, !isTrashing, !isScanning else { return }
        isTrashing = true
        Task {
            let results = await Task.detached(priority: .userInitiated) {
                entries.map { entry in (entry: entry, failure: Self.moveToTrash(entry)) }
            }.value
            finishTrash(results)
        }
    }

    /// Returns nil once the item is in the Trash, or why it isn't.
    private nonisolated static func moveToTrash(_ entry: ScanEntry) -> String? {
        let url = URL(filePath: entry.path, directoryHint: entry.isDirectory ? .isDirectory : .notDirectory)
        do {
            // The scan saw this kind of item. Don't move something else that took its place.
            let isDirectory = try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory ?? false
            guard isDirectory == entry.isDirectory else { return "It changed since the scan." }
            try FileManager.default.trashItem(at: url, resultingItemURL: nil)
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    private func finishTrash(_ results: [(entry: ScanEntry, failure: String?)]) {
        isTrashing = false
        let moved = results.filter { $0.failure == nil }.map(\.entry)
        if !moved.isEmpty, let report {
            acceptReport(report.removing(Set(moved.map(\.path))))
        }
        let freed = moved.reduce(UInt64(0)) { $0 + $1.allocatedBytes }
        let what = moved.count == 1 ? "“\(moved[0].name)”" : ScanFormat.items(moved.count)
        if let failed = results.first(where: { $0.failure != nil }) {
            let failures = results.count - moved.count
            notice = Notice(
                title: moved.isEmpty
                    ? "Couldn’t move “\(failed.entry.name)” to the Trash"
                    : "Moved \(moved.count) of \(results.count) items to the Trash",
                message: (failures > 1 ? "\(failures) items stayed. " : "")
                    + "“\(failed.entry.name)”: \(failed.failure ?? "")",
                isError: true
            )
        } else {
            notice = Notice(
                title: "Moved \(what) to the Trash",
                message: "\(ScanFormat.size(freed)). Space is freed when you empty the Trash."
            )
        }
    }

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
            let rowPaths = Set(rows.map(\.path))
            if !selectedPaths.isSubset(of: rowPaths) { selectedPaths.formIntersection(rowPaths) }
            selectedEntry = selection.flatMap(entry(withPath:))
            isLoadingRows = false
            rowsTask = nil
        }
    }
}
