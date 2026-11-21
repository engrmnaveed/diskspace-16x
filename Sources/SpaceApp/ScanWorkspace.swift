import AppKit
import Observation
import SpaceCore
import SpacePro
import UniformTypeIdentifiers

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
    /// Pro: the folder's file types.
    case types = "Types"

    /// The views the Contents picker offers. Types needs **Settings → Pro features**.
    static func available(pro: Bool) -> [FolderPresentation] {
        pro ? allCases : allCases.filter { $0 != .types }
    }
}

/// A sortable table column. `.reverse` means largest first for sizes and counts, Z–A for names.
struct EntrySort: SortComparator, Hashable {
    var key: EntryOrder
    var order: SortOrder
    /// Ranks by the change since a saved scan instead, for Pro's Compare.
    var byChange = false

    static let largestFirst = EntrySort(key: .size, order: .reverse)
    static let biggestGrowth = EntrySort(key: .size, order: .reverse, byChange: true)

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
    /// The saved scan being opened, if any.
    private(set) var openingFile: String?
    /// The file the results came from, when they were opened rather than scanned. Its
    /// items may be elsewhere or may have changed, so the results are read-only.
    private(set) var importedFile: String?
    /// The exclude patterns the results were scanned with, for Export Scan.
    private(set) var reportExcludes: [String] = []
    /// Pro: saved scans of the current root in the history, newest first.
    private(set) var savedScans: [SavedScanRecord] = []
    /// Mounted volumes for the start screen.
    private(set) var volumes: [VolumeSummary] = []
    /// Pro: the saved scan the results are compared with, and the open folder's changes.
    private(set) var comparison: ScanComparison?
    private(set) var folderChanges: FolderChanges?
    private var isPresentingPicker = false
    private(set) var notice: Notice?
    private(set) var selectedEntry: ScanEntry?
    var showsDetails = false
    /// Pro: the figures the Explain Disk sheet shows, while it is open.
    var explanation: DiskExplanation?
    private(set) var isExplaining = false
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
            // Quick Look follows the selection while it is open, as in Finder.
            if quickLookURL != nil {
                quickLookURL = selectedEntry.flatMap { entry in
                    entry.isCloudOnly ? nil : URL(filePath: entry.path)
                }
            }
        }
    }

    /// The one selected item, for Open, Reveal, Copy Path, and the charts. Nil when
    /// several items are selected.
    var selection: String? {
        get { selectedPaths.count == 1 ? selectedPaths.first : nil }
        set { selectedPaths = newValue.map { [$0] } ?? [] }
    }

    var selectedEntries: [ScanEntry] { rows.filter { selectedPaths.contains($0.path) } }

    /// The item Quick Look shows. Quick Look reads the file only while it is open.
    var quickLookURL: URL?
    var canQuickLook: Bool { canBrowse && selectedEntry != nil && !isImported }

    /// Opens Quick Look for the selected item, or closes it. Cloud-only items stay closed,
    /// because previewing them would download them.
    func toggleQuickLook() {
        if quickLookURL != nil {
            quickLookURL = nil
            return
        }
        guard canBrowse, let entry = selectedEntry else { return }
        if entry.isCloudOnly {
            notice = Notice(
                title: "“\(entry.name)” is cloud-only",
                message: "Quick Look would download it. Open it in Finder if you want to.",
                isError: true
            )
            return
        }
        quickLookURL = URL(filePath: entry.path, directoryHint: entry.isDirectory ? .isDirectory : .notDirectory)
    }

    /// Items waiting for the user to confirm Move to Trash. A nonempty list shows the alert.
    var pendingTrash: [ScanEntry] = []
    private(set) var isTrashing = false
    var canMoveToTrash: Bool { canBrowse && !isScanning && !isTrashing && !selectedPaths.isEmpty && !isImported }

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
    var canChooseFolder: Bool { !isScanning && !isPresentingPicker && openingFile == nil }
    var canBrowse: Bool { report != nil && (!isScanning || scanHasResults) }
    var isImported: Bool { importedFile != nil }
    var canRescan: Bool { report != nil && canChooseFolder && !isImported }
    var canReveal: Bool { canBrowse && !isImported }
    var canExport: Bool { canBrowse && canChooseFolder }
    var canCompare: Bool { canBrowse && canChooseFolder && ProFeatures.isEnabled }
    var canSaveToHistory: Bool { canExport && ProFeatures.isEnabled }

    /// Suggests Full Disk Access only when macOS privacy controls kept items out of this scan.
    var accessAdvice: AccessAdvice {
        guard hasPrivacyRefusals, !isImported else { return .none }
        if !hasFullDiskAccess { return .grant }
        return scanHadFullDiskAccess ? .none : .rescan
    }

    func refreshVolumes() {
        Task {
            let mounted = await Task.detached(priority: .utility) { VolumeSummary.mounted() }.value
            if mounted != volumes { volumes = mounted }
        }
    }

    /// Pro: explains the space on the volume of the scanned folder, or of `path`.
    func explainDisk(at path: String? = nil) {
        guard ProFeatures.isEnabled, !isExplaining else { return }
        let path = path ?? report?.root.path ?? "/"
        isExplaining = true
        Task {
            let result = await Task.detached(priority: .userInitiated) {
                DiskExplanation.explain(URL(filePath: path))
            }.value
            isExplaining = false
            if let result {
                explanation = result
            } else {
                notice = Notice(
                    title: "Couldn't explain this disk",
                    message: "macOS didn't report the volume of “\(path)”.",
                    isError: true
                )
            }
        }
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
        if url.pathExtension.lowercased() == "json", !url.hasDirectoryPath {
            openScan(at: url)
        } else {
            startScan(url)
        }
        return true
    }

    /// Asks for a saved scan to open, read-only.
    func openScan() {
        guard canChooseFolder else { return }
        let picker = NSOpenPanel()
        picker.title = "Open a Saved Scan"
        picker.message = "Choose a scan exported by Space 16X, the space16x CLI, ncdu, or gdu."
        picker.prompt = "Open"
        picker.allowedContentTypes = [.json]
        picker.allowsMultipleSelection = false
        isPresentingPicker = true
        Task {
            let result = await picker.begin()
            isPresentingPicker = false
            if result == .OK, let url = picker.url { openScan(at: url) }
        }
    }

    /// Opens a scan saved in ncdu's JSON format, by Space 16X or another tool.
    func openScan(at url: URL) {
        guard canChooseFolder else { return }
        let name = url.lastPathComponent
        openingFile = name
        notice = nil
        let didAccess = url.startAccessingSecurityScopedResource()
        Task {
            let result = await Task.detached(priority: .userInitiated) {
                defer { if didAccess { url.stopAccessingSecurityScopedResource() } }
                return Result { try SavedScan(contentsOf: url) }
            }.value
            openingFile = nil
            switch result {
            case let .success(saved):
                scanHasResults = false
                showsDetails = false
                acceptReport(saved.report, from: .file(name: name, excludes: saved.excludes))
                notice = Notice(
                    title: "Opened “\(name)”",
                    message: "Saved scans are read-only. Scan the folder again to rescan it or move items to the Trash."
                )
            case let .failure(error):
                notice = Notice(
                    title: "Couldn’t open “\(name)”",
                    message: error.localizedDescription,
                    isError: true
                )
            }
        }
    }

    /// Pro: asks for a saved scan of this folder and shows what changed since then.
    func compareWithSavedScan() {
        guard canCompare else { return }
        let picker = NSOpenPanel()
        picker.title = "Compare with a Saved Scan"
        picker.message = "Choose an earlier scan of this folder, exported by Space 16X, the space16x CLI, ncdu, or gdu."
        picker.prompt = "Compare"
        picker.allowedContentTypes = [.json]
        picker.allowsMultipleSelection = false
        isPresentingPicker = true
        Task {
            let result = await picker.begin()
            isPresentingPicker = false
            if result == .OK, let url = picker.url { compare(with: url) }
        }
    }

    /// Pro: compares the results with a scan saved in the history.
    func compare(with record: SavedScanRecord) {
        let name = record.date.formatted(date: .abbreviated, time: .shortened)
        compare(with: ScanHistory.standard.url(for: record), name: name)
    }

    /// Pro: compares the results with the saved scan at `url`, called `name` in messages.
    func compare(with url: URL, name: String? = nil) {
        guard canCompare else { return }
        let name = name ?? url.lastPathComponent
        Task {
            let loaded = await Task.detached(priority: .userInitiated) {
                Result { try SavedScan(contentsOf: url) }
            }.value
            switch loaded {
            case let .success(saved):
                comparison = ScanComparison(name: name, baseline: saved.report)
                notice = Notice(
                    title: "Comparing with “\(name)”",
                    message: "The Change column shows what grew or shrank since that scan. Click its header to rank by change."
                )
                loadRows(keepingRows: true)
            case let .failure(error):
                notice = Notice(title: "Couldn’t open “\(name)”", message: error.localizedDescription, isError: true)
            }
        }
    }

    /// Leaves Pro's sort orders when **Settings → Pro features** turns off.
    func stopProFeatures() {
        stopComparing()
        if folderPresentation == .types {
            folderPresentation = .list
        }
        if sortOrder.first?.key == .modified {
            sortOrder = [.largestFirst]
        }
    }

    func stopComparing() {
        guard comparison != nil else { return }
        comparison = nil
        folderChanges = nil
        if sortOrder.first?.byChange == true {
            sortOrder = [.largestFirst]
        }
    }

    /// Pro: keeps the results in the history, to compare with later.
    func saveToHistory() {
        guard canSaveToHistory, let report else { return }
        let saved = SavedScan(report: report, excludes: reportExcludes)
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
        Task {
            let outcome = await Task.detached(priority: .userInitiated) {
                Result { try ScanHistory.standard.save(saved, version: version) }
            }.value
            switch outcome {
            case .success:
                notice = Notice(
                    title: "Saved to History",
                    message: "Compare… lists this scan. Saved scans are in ~/Library/Application Support/Space 16X/History."
                )
                refreshSavedScans()
            case let .failure(error):
                notice = Notice(title: "Couldn’t save the scan", message: error.localizedDescription, isError: true)
            }
        }
    }

    /// Pro: reads the history's saved scans of the current root.
    func refreshSavedScans() {
        guard ProFeatures.isEnabled, let root = report?.root.path else {
            savedScans = []
            return
        }
        Task {
            let records = await Task.detached(priority: .utility) {
                ScanHistory.standard.records(of: root)
            }.value
            if report?.root.path == root { savedScans = records }
        }
    }

    /// Opens the history folder in Finder.
    func showHistoryInFinder() {
        let folder = ScanHistory.standard.directory
        if FileManager.default.fileExists(atPath: folder.path) {
            NSWorkspace.shared.activateFileViewerSelecting([folder])
        }
    }

    /// Saves the results in ncdu's JSON format, which the CLI, ncdu, and gdu also open.
    func exportScan() {
        guard canExport, let report else { return }
        let saved = SavedScan(report: report, excludes: reportExcludes)
        let panel = NSSavePanel()
        panel.title = "Export Scan"
        panel.message = "Open it later with Open Scan…, space16x --import, ncdu -f, or gdu -f. It lists every path in the scan."
        panel.prompt = "Export"
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "\(report.root.path == "/" ? "Startup Disk" : report.root.name) scan.json"
        isPresentingPicker = true
        Task {
            let result = await panel.begin()
            isPresentingPicker = false
            guard result == .OK, let url = panel.url else { return }
            let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
            let outcome = await Task.detached(priority: .userInitiated) {
                Result { try saved.exportData(version: version).write(to: url, options: .atomic) }
            }.value
            switch outcome {
            case .success:
                notice = Notice(
                    title: "Exported “\(url.lastPathComponent)”",
                    message: "Review saved scans for personal paths before you share them."
                )
            case let .failure(error):
                notice = Notice(
                    title: "Couldn’t export the scan",
                    message: error.localizedDescription,
                    isError: true
                )
            }
        }
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
            return try await DiskScanner.rescan(
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
        let patterns = ExcludeSettings.patterns
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
            return try await DiskScanner.scan(
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
                    self.acceptReport(snapshot, from: .scan(excludes: patterns))
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
                self?.acceptReport(result, from: .scan(excludes: patterns))
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
        guard !isImported else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: entry.path)])
    }

    func copyPath(of entry: ScanEntry) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(entry.path, forType: .string)
    }

    /// Where new results come from. Results that only change keep their source.
    private enum ReportSource {
        case scan(excludes: [String])
        case file(name: String, excludes: [String])
    }

    private func acceptReport(_ result: ScanReport, from source: ReportSource? = nil) {
        switch source {
        case let .scan(excludes):
            importedFile = nil
            reportExcludes = excludes
        case let .file(name, excludes):
            importedFile = name
            reportExcludes = excludes
        case nil:
            break
        }
        let newRoot = report?.root.path != result.root.path
        let continuing = scanHasResults
        scanHasResults = true
        report = result
        if newRoot { refreshSavedScans() }
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
        let comparison = mode == .folders ? comparison : nil
        let names = scope.dropFirst().map(\.name)
        let requested = sortOrder.first ?? .largestFirst
        let sort = requested.byChange && comparison == nil ? .largestFirst : requested
        rowsTask = Task { [weak self] in
            // Let a running sort release its buffers before allocating for a newer ranking.
            await previousTask?.value
            guard !Task.isCancelled else { return }
            let worker = Task.detached(priority: .userInitiated) {
                let ranked = switch mode {
                case .folders: folder.rankedChildren(by: metric)
                case .largestFiles: report.largestFiles(limit: Self.largestFileLimit, by: metric)
                }
                let changes = comparison?.changes(in: folder, at: names, metric: metric)
                let rows: [ScanEntry]
                if sort.byChange, let changes {
                    // Largest growth first, or largest shrink first in ascending order.
                    rows = ranked.sorted { left, right in
                        let (a, b) = (changes.delta(for: left, metric: metric), changes.delta(for: right, metric: metric))
                        if a == b { return left.name.utf8.lexicographicallyPrecedes(right.name.utf8) }
                        return sort.order == .forward ? a < b : a > b
                    }
                } else if sort == .largestFirst {
                    rows = ranked
                } else {
                    rows = ranked.ordered(by: sort.key, ascending: sort.order == .forward, metric: metric)
                }
                return (rows: rows, chart: Array(ranked.prefix(32)), changes: changes)
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
            folderChanges = result.changes
            let rowPaths = Set(rows.map(\.path))
            if !selectedPaths.isSubset(of: rowPaths) { selectedPaths.formIntersection(rowPaths) }
            selectedEntry = selection.flatMap(entry(withPath:))
            isLoadingRows = false
            rowsTask = nil
        }
    }
}
