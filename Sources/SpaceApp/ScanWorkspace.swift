import AppKit
import Observation
import SpaceCore
import SpacePro
import UniformTypeIdentifiers

enum BrowseMode: String, CaseIterable {
    case folders = "Folders"
    case largestFiles = "Largest Files"
    /// Pro: folders that tools recreate, for Move to Trash.
    case cleanup = "Cleanup"
    /// Pro: files with the same contents, once the user asks to compare them.
    case duplicates = "Duplicates"

    var symbol: String {
        switch self {
        case .folders: "folder"
        case .largestFiles: "list.number"
        case .cleanup: "sparkles"
        case .duplicates: "doc.on.doc"
        }
    }

    /// The modes the sidebar offers. Cleanup and Duplicates need **Settings → Pro features**.
    static func available(pro: Bool) -> [BrowseMode] {
        pro ? allCases : [.folders, .largestFiles]
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
    /// Ranks by exclusive size instead, for Pro's Exclusive column.
    var byExclusive = false

    static let largestFirst = EntrySort(key: .size, order: .reverse)
    static let biggestGrowth = EntrySort(key: .size, order: .reverse, byChange: true)
    static let mostExclusive = EntrySort(key: .size, order: .reverse, byExclusive: true)

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
    /// Changes when the folder list should be rebuilt with the new rows instead of updated in
    /// place, after the user rearranges them. See `loadRows(keepingRows:rearranging:)`.
    private(set) var tableGeneration = 0
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
    /// When the results were scanned, for Build Footprint receipts. Nil for opened scans.
    private(set) var scannedAt: Date?
    /// Pro: the sheet that names a Build Footprint receipt before saving it.
    var showsReceiptSheet = false
    var canSaveReceipt: Bool { comparison != nil && canExport && ProFeatures.isEnabled }
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
    /// Pro: a note for each row of Cleanup or Duplicates, by path.
    private(set) var rowLabels: [String: String] = [:]
    /// Pro: Cleanup rows to review before removing, which can't be recreated.
    private(set) var reviewPaths: Set<String> = []
    /// Pro: files with the same contents, or nil until the user asks to find them.
    private(set) var duplicateGroups: [DuplicateGroup]?
    private(set) var isFindingDuplicates = false
    /// Pro: the Exclusive column, from **View → Show Exclusive Size**. Turning it on measures
    /// the finished scan in the background, so scans stay as fast as before.
    var showsExclusive = false {
        didSet {
            guard showsExclusive != oldValue else { return }
            if !showsExclusive, sortOrder.first?.byExclusive == true { sortOrder = [.largestFirst] }
            measureExclusiveSizes()
        }
    }
    /// Pro: **View → Show Files Older Than**, in days, or nil for files of any age. The
    /// results then count only files last modified before then, as `scan --older-than` does.
    var olderThanDays: Int? {
        didSet { if olderThanDays != oldValue { applyAgeFilter() } }
    }
    /// The latest results before the age filter, which saving, exporting a scan, rescanning
    /// and Move to Trash work from.
    private var unfilteredReport: ScanReport?
    /// Whether the shown results leave out files newer than the age filter.
    var isAgeFiltered: Bool {
        olderThanDays != nil && ProFeatures.isEnabled && importedFile == nil && report?.isPreview == false
    }

    /// Pro: the current scan's exclusive sizes, once measured.
    private(set) var exclusiveSizes: ExclusiveSizes?
    /// Pro: the share of the scan measured so far, from 0 to 1, while measuring.
    private(set) var exclusiveProgress: Double?
    private var exclusiveTask: Task<Void, Never>?
    private var exclusiveGeneration = 0

    var sizeMetric: SizeMetric = SizeMetric(rawValue: UserDefaults.standard.string(forKey: "sizeMetric") ?? "")
        ?? .allocated
    {
        didSet {
            guard sizeMetric != oldValue else { return }
            UserDefaults.standard.set(sizeMetric.rawValue, forKey: "sizeMetric")
            loadRows(keepingRows: true, rearranging: true)
        }
    }

    var sortOrder: [EntrySort] = [.largestFirst] {
        didSet {
            guard sortOrder != oldValue else { return }
            loadRows(keepingRows: true, rearranging: true)
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
    /// Pro: the bytes that emptying the Trash frees at least for `pendingTrash`, without
    /// blocks shared with clones or files with other hard links, when measured in time.
    private(set) var pendingExclusiveBytes: UInt64?
    /// Pro: a plain warning when `pendingTrash` is or holds folders that can't be recreated,
    /// such as Xcode archives.
    private(set) var pendingReviewWarning: String?
    private(set) var isMeasuringTrash = false
    private(set) var isTrashing = false
    var canMoveToTrash: Bool {
        canBrowse && !isScanning && !isTrashing && !isMeasuringTrash && !selectedPaths.isEmpty && !isImported
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
    var canChooseFolder: Bool { !isScanning && !isPresentingPicker && openingFile == nil }
    var canBrowse: Bool { report != nil && (!isScanning || scanHasResults) }
    var isImported: Bool { importedFile != nil }
    var canRescan: Bool { report != nil && canChooseFolder && !isImported }
    var canReveal: Bool { canBrowse && !isImported }
    var canExport: Bool { canBrowse && canChooseFolder }
    var canCompare: Bool { canBrowse && canChooseFolder && ProFeatures.isEnabled && !isAgeFiltered }
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
        compare(with: ScanHistory.standard.url(for: record), name: name, date: record.date)
    }

    /// Pro: compares the results with the saved scan at `url`, called `name` in messages.
    /// `date` is when it was made, or else the file's modification date.
    func compare(with url: URL, name: String? = nil, date: Date? = nil) {
        guard canCompare else { return }
        let name = name ?? url.lastPathComponent
        Task {
            let loaded = await Task.detached(priority: .userInitiated) {
                Result { try SavedScan(contentsOf: url) }
            }.value
            let saved = date ?? (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
            switch loaded {
            case let .success(scan):
                comparison = ScanComparison(name: name, baseline: scan.report, date: saved)
                notice = Notice(
                    title: "Comparing with “\(name)”",
                    message: "The Change column shows what grew or shrank since that scan. Click its header to rank by change."
                )
                loadRows(keepingRows: true, rearranging: true)
            case let .failure(error):
                notice = Notice(title: "Couldn’t open “\(name)”", message: error.localizedDescription, isError: true)
            }
        }
    }

    /// Leaves Pro's sort orders when **Settings → Pro features** turns off.
    func stopProFeatures() {
        stopComparing()
        showsExclusive = false
        olderThanDays = nil
        if mode == .cleanup || mode == .duplicates {
            mode = .folders
        }
        if folderPresentation == .types {
            folderPresentation = .list
        }
        if sortOrder.first?.key == .modified {
            sortOrder = [.largestFirst]
        }
    }

    /// Pro: shows the latest results again through the age filter. Comparing ends, since a
    /// saved scan counts files of every age.
    private func applyAgeFilter() {
        guard let unfilteredReport else { return }
        if olderThanDays != nil { stopComparing() }
        acceptReport(unfilteredReport, rearranging: true)
        if olderThanDays != nil, importedFile != nil {
            notice = Notice(
                title: "Opened scans don’t have modification dates",
                message: "Scan the folder to show only older files."
            )
        }
    }

    /// The results with only files older than the age filter, when it's on.
    private func ageFiltered(_ result: ScanReport) -> ScanReport {
        guard let days = olderThanDays, ProFeatures.isEnabled, importedFile == nil, !result.isPreview else { return result }
        return OlderThan.filter(result, cutoff: OlderThan.cutoff(days: days))
    }

    /// Pro: measures the finished scan's exclusive sizes while the Exclusive column is shown,
    /// replacing any earlier measurement. Opened scans can't be measured: their files may be
    /// elsewhere.
    private func measureExclusiveSizes(reloadingRows: Bool = true) {
        exclusiveTask?.cancel()
        exclusiveTask = nil
        exclusiveGeneration += 1
        let hadSizes = exclusiveSizes != nil
        exclusiveSizes = nil
        exclusiveProgress = nil
        if reloadingRows, hadSizes, sortOrder.first?.byExclusive == true { loadRows(keepingRows: true, rearranging: true) }
        guard showsExclusive, ProFeatures.isEnabled, importedFile == nil, let report, !report.isPreview else { return }
        let generation = exclusiveGeneration
        exclusiveProgress = 0
        let showProgress: @Sendable (Double) -> Void = { [self] fraction in
            Task { @MainActor in
                guard exclusiveGeneration == generation, exclusiveSizes == nil else { return }
                exclusiveProgress = fraction
            }
        }
        exclusiveTask = Task { [weak self] in
            let worker = Task.detached(priority: .utility) {
                await ExclusiveSizes.measure(report, progress: showProgress)
            }
            let sizes = await withTaskCancellationHandler {
                await worker.value
            } onCancel: {
                worker.cancel()
            }
            guard let self, !Task.isCancelled, self.exclusiveGeneration == generation, let sizes else { return }
            self.exclusiveSizes = sizes
            self.exclusiveProgress = nil
            self.exclusiveTask = nil
            if self.sortOrder.first?.byExclusive == true { self.loadRows(keepingRows: true, rearranging: true) }
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
        guard canSaveToHistory, let report = unfilteredReport ?? report else { return }
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
    /// Pro: saves a Build Footprint receipt of the comparison, labelled `label`, as Markdown.
    func saveReceipt(label: String, budget: UInt64? = nil) {
        guard canSaveReceipt, let comparison, let report else { return }
        let panel = NSSavePanel()
        panel.title = "Save Footprint Receipt"
        panel.message = "The receipt lists what changed since “\(comparison.name)”, with paths relative to the scanned folder."
        panel.prompt = "Save"
        panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        panel.nameFieldStringValue = "Footprint – \(trimmed.isEmpty ? report.root.name : trimmed).md"
        let metric = sizeMetric
        let scannedAt = scannedAt
        isPresentingPicker = true
        Task {
            let result = await panel.begin()
            isPresentingPicker = false
            guard result == .OK, let url = panel.url else { return }
            let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
            let outcome = await Task.detached(priority: .userInitiated) {
                Result {
                    let text = FootprintReceipt.markdown(
                        label: label, comparison: comparison, after: report, afterDate: scannedAt,
                        metric: metric, version: version, budget: budget
                    )
                    try Data(text.utf8).write(to: url, options: .atomic)
                }
            }.value
            switch outcome {
            case .success:
                notice = Notice(title: "Saved “\(url.lastPathComponent)”", message: "Open it in any text or Markdown editor.")
            case let .failure(error):
                notice = Notice(title: "Couldn’t save the receipt", message: error.localizedDescription, isError: true)
            }
        }
    }

    func exportScan() {
        guard canExport, let report else { return }
        let saved = SavedScan(report: unfilteredReport ?? report, excludes: reportExcludes)
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

    /// Pro: saves one self-contained HTML page with the totals, a treemap, the scan root's
    /// contents, the largest files and file types, as `space16x scan --html` writes it.
    func exportHTMLReport() {
        guard canExport, ProFeatures.isEnabled, let report else { return }
        let panel = NSSavePanel()
        panel.title = "Export HTML Report"
        panel.message = "The page lists the scan’s largest items and files with their paths. It has no scripts and loads nothing."
        panel.prompt = "Export"
        panel.allowedContentTypes = [.html]
        panel.nameFieldStringValue = "\(report.root.path == "/" ? "Startup Disk" : report.root.name) report.html"
        let metric = sizeMetric
        let excludes = reportExcludes
        let days = isAgeFiltered ? olderThanDays : nil
        let scannedAt = scannedAt ?? Date()
        isPresentingPicker = true
        Task {
            let result = await panel.begin()
            isPresentingPicker = false
            guard result == .OK, let url = panel.url else { return }
            let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
            let outcome = await Task.detached(priority: .userInitiated) {
                Result {
                    let page = HTMLReport.page(
                        report: report, metric: metric, excludes: excludes, olderThanDays: days,
                        version: version, scannedAt: scannedAt
                    )
                    try Data(page.utf8).write(to: url, options: .atomic)
                }
            }.value
            switch outcome {
            case .success:
                notice = Notice(
                    title: "Exported “\(url.lastPathComponent)”",
                    message: "Open it in any browser. Review the paths on the page before you share it."
                )
            case let .failure(error):
                notice = Notice(title: "Couldn’t export the report", message: error.localizedDescription, isError: true)
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
        guard let merged = (unfilteredReport ?? report)?.replacing(result) else {
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
        } else if let report = unfilteredReport ?? report,
                  (try? URL(filePath: folder.path).checkResourceIsReachable()) != true {
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
        // A folder in the age-filtered results may hold newer files the list leaves out.
        if isAgeFiltered, let folder = entries.first(where: \.isDirectory) {
            notice = Notice(
                title: "“\(folder.name)” may hold newer files",
                message: "The list shows only older files. Choose View → Show Files Older Than → Any Age to move folders to the Trash.",
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
        guard ProFeatures.isEnabled else {
            pendingExclusiveBytes = nil
            pendingReviewWarning = nil
            pendingTrash = entries
            return
        }
        // Pro reads how much of the items is shared before asking, for up to two seconds,
        // and warns about folders that can't be recreated.
        isMeasuringTrash = true
        Task {
            let (measured, warning) = await Task.detached(priority: .userInitiated) {
                (ExclusiveSize.measure(entries), CleanupSuggestions.reviewWarning(for: entries))
            }.value
            isMeasuringTrash = false
            pendingExclusiveBytes = measured
            pendingReviewWarning = warning
            pendingTrash = entries
        }
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
            let trashed = Set(moved.map(\.path))
            // Duplicate groups keep the copies that are still there.
            let remaining = duplicateGroups?.compactMap { group in
                group.keeping { file in !trashed.contains { file.path == $0 || file.path.hasPrefix($0 + "/") } }
            }
            acceptReport((unfilteredReport ?? report).removing(trashed), duplicates: remaining)
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

    /// Pro: compares the files in the finished scan that share a length, by reading them,
    /// and lists the groups with the same contents. Only this reads file contents.
    func findDuplicates() {
        guard ProFeatures.isEnabled, let report, !report.isPreview, !isScanning, !isFindingDuplicates else { return }
        mode = .duplicates
        isFindingDuplicates = true
        Task {
            let found = await Task.detached(priority: .userInitiated) {
                DuplicateFinder.find(in: report)
            }.value
            isFindingDuplicates = false
            // A newer scan replaced the results meanwhile.
            guard self.report?.root.path == report.root.path, !isScanning else { return }
            duplicateGroups = found.groups
            if !found.unreadable.isEmpty {
                notice = Notice(
                    title: "Some files couldn't be compared",
                    message: "\(ScanFormat.items(found.unreadable.count)) couldn't be read, so they aren't in the groups.",
                    isError: true
                )
            }
            loadRows(keepingRows: true, rearranging: true)
        }
    }

    /// Shows new results. `rearranging` rebuilds the folder list when its rows change, for a
    /// change the user made, such as the age filter, rather than results arriving from a scan.
    private func acceptReport(
        _ incoming: ScanReport, from source: ReportSource? = nil, duplicates: [DuplicateGroup]? = nil,
        rearranging: Bool = false
    ) {
        duplicateGroups = duplicates
        switch source {
        case let .scan(excludes):
            importedFile = nil
            reportExcludes = excludes
            scannedAt = Date()
        case let .file(name, excludes):
            importedFile = name
            reportExcludes = excludes
            scannedAt = nil
        case nil:
            break
        }
        unfilteredReport = incoming
        let result = ageFiltered(incoming)
        let newRoot = report?.root.path != result.root.path
        let continuing = scanHasResults
        scanHasResults = true
        report = result
        if newRoot { refreshSavedScans() }
        if showsExclusive || exclusiveSizes != nil { measureExclusiveSizes(reloadingRows: false) }
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
            let sameLocation = mode != .folders || oldScope.last?.id == scope.last?.id
            if !sameLocation { selection = nil }
            loadRows(keepingRows: sameLocation, rearranging: rearranging)
        } else if mode != .folders {
            mode = .folders
        } else {
            selection = nil
            loadRows()
        }
    }

    /// Ranks the open folder's rows in the background. `keepingRows` leaves the current rows
    /// on screen until the new ones are ready, so the list doesn't blank for a re-sort.
    ///
    /// `rearranging` marks a change the user made, such as a new sort order. When it puts the
    /// rows in a different order, the folder list is rebuilt with them instead of updated in
    /// place: when rows are inserted or moved in more than one place, NSTableView measures
    /// their heights reentrantly and logs a warning that it will become an assertion.
    /// Results streaming in from a scan update the list in place, keeping its scroll position.
    private func loadRows(keepingRows: Bool = false, rearranging: Bool = false) {
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
        let duplicates = duplicateGroups
        let exclusive = exclusiveSizes
        let names = scope.dropFirst().map(\.name)
        let previousRows = rearranging ? rows : []
        let requested = sortOrder.first ?? .largestFirst
        let sort = (requested.byChange && comparison == nil) || (requested.byExclusive && exclusive == nil)
            ? .largestFirst : requested
        rowsTask = Task { [weak self] in
            // Let a running sort release its buffers before allocating for a newer ranking.
            await previousTask?.value
            guard !Task.isCancelled else { return }
            let worker = Task.detached(priority: .userInitiated) {
                var labels: [String: String] = [:]
                var review: Set<String> = []
                let ranked: [ScanEntry]
                switch mode {
                case .folders: ranked = folder.rankedChildren(by: metric)
                case .largestFiles: ranked = report.largestFiles(limit: Self.largestFileLimit, by: metric)
                case .cleanup:
                    // Suggestions come from a finished scan, never a preview.
                    let found = report.isPreview ? [] : CleanupSuggestions.find(in: report, metric: metric)
                    ranked = found.map(\.entry)
                    for suggestion in found {
                        labels[suggestion.entry.path] = suggestion.rule.review == nil
                            ? suggestion.rule.label
                            : suggestion.rule.label + " · review, can't be recreated"
                        if suggestion.rule.review != nil { review.insert(suggestion.entry.path) }
                    }
                case .duplicates:
                    ranked = duplicates?.flatMap(\.files) ?? []
                    for group in duplicates ?? [] {
                        for (index, file) in group.files.enumerated() {
                            // A clone says what removing it frees, short enough for a narrow column.
                            if group.isClone(at: index) {
                                let frees = group.exclusiveBytes(at: index)
                                let size = ByteCountFormatter.string(fromByteCount: Int64(clamping: frees), countStyle: .file)
                                labels[file.path] = frees == 0 ? "Clone, frees nothing" : "Clone, frees \(size)"
                            } else {
                                labels[file.path] = "Copy \(index + 1) of \(group.files.count)"
                            }
                        }
                    }
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
                } else if sort.byExclusive, let exclusive {
                    // Most exclusive first, or least first in ascending order.
                    rows = ranked.sorted { left, right in
                        let (a, b) = (exclusive.bytes(for: left), exclusive.bytes(for: right))
                        if a == b { return left.name.utf8.lexicographicallyPrecedes(right.name.utf8) }
                        return sort.order == .forward ? a < b : a > b
                    }
                } else if sort == .largestFirst {
                    rows = ranked
                } else {
                    rows = ranked.ordered(by: sort.key, ascending: sort.order == .forward, metric: metric)
                }
                let rearranged = !previousRows.isEmpty && !previousRows.elementsEqual(rows) { $0.path == $1.path }
                return (
                    rows: rows, chart: Array(ranked.prefix(32)), changes: changes, labels: labels, review: review,
                    rearranged: rearranged
                )
            }
            let result = await withTaskCancellationHandler {
                await worker.value
            } onCancel: {
                worker.cancel()
            }
            guard !Task.isCancelled, let self else { return }
            rankedScope = mode == .folders ? folder : report.root
            if result.rearranged { tableGeneration &+= 1 }
            rows = result.rows
            chartRows = result.chart
            folderChanges = result.changes
            rowLabels = result.labels
            reviewPaths = result.review
            let rowPaths = Set(rows.map(\.path))
            if !selectedPaths.isSubset(of: rowPaths) { selectedPaths.formIntersection(rowPaths) }
            selectedEntry = selection.flatMap(entry(withPath:))
            isLoadingRows = false
            rowsTask = nil
        }
    }
}
