import SpaceCore
import SpacePro
import QuickLook
import SwiftUI

struct ResultsView: View {
    @Bindable var workspace: ScanWorkspace
    @AppStorage(ProFeatures.defaultsKey) private var proFeatures = false
    /// Shows the Change column only while comparing. Conditional columns need macOS 14.4.
    @ViewState private var columns = TableColumnCustomization<ScanEntry>()

    /// Pro's Compare adds a Change column to the folder list.
    private var showsChanges: Bool {
        proFeatures && workspace.comparison != nil && workspace.mode == .folders
    }

    /// Pro's **View → Show Exclusive Size** adds an Exclusive column.
    private var showsExclusive: Bool {
        proFeatures && workspace.showsExclusive
    }

    var body: some View {
        if let scope = workspace.visibleScope {
            VStack(spacing: 0) {
                scopeHeader(scope)
                scanNotice
                if workspace.mode == .folders {
                    presentationBar
                }
                let rankedScope = workspace.rankedScope ?? scope
                switch workspace.mode == .folders ? workspace.folderPresentation : .list {
                case .list: resultsTable(scope: rankedScope)
                case .treemap: DiskTreemapView(workspace: workspace, folder: rankedScope)
                case .donut: DiskDonutView(workspace: workspace, folder: rankedScope)
                case .types: FileTypesView(workspace: workspace, folder: rankedScope)
                }
                selectionBar
            }
            .alert(trashTitle, isPresented: trashAlertShown) {
                Button("Move to Trash", role: .destructive, action: workspace.confirmTrash)
                Button("Cancel", role: .cancel) {}
            } message: {
                let bytes = workspace.pendingTrash.reduce(UInt64(0)) { $0 + $1.allocatedBytes }
                let warning = workspace.pendingReviewWarning.map { "\($0) " } ?? ""
                if let exclusive = workspace.pendingExclusiveBytes, exclusive < bytes {
                    Text("\(warning)\(ScanFormat.size(bytes)) of allocated space. Emptying the Trash frees at least \(ScanFormat.size(exclusive)), because clones or other hard links share the rest.")
                } else {
                    Text("\(warning)\(ScanFormat.size(bytes)) of allocated space. Space is freed when you empty the Trash.")
                }
            }
            .onChange(of: proFeatures, initial: true) { _, enabled in
                if !enabled { workspace.stopProFeatures() }
                workspace.refreshSavedScans()
                columns[visibility: "modified"] = enabled ? .visible : .hidden
            }
            .onChange(of: showsChanges, initial: true) { _, shown in
                columns[visibility: "change"] = shown ? .visible : .hidden
            }
            .onChange(of: showsExclusive, initial: true) { _, shown in
                columns[visibility: "exclusive"] = shown ? .visible : .hidden
            }
        }
    }

    private var trashTitle: String {
        let items = workspace.pendingTrash
        return items.count == 1 ? "Move “\(items[0].name)” to the Trash?" : "Move \(items.count) items to the Trash?"
    }

    private var trashAlertShown: Binding<Bool> {
        Binding(
            get: { !workspace.pendingTrash.isEmpty },
            set: { if !$0 { workspace.pendingTrash = [] } }
        )
    }

    private func scopeHeader(_ scope: ScanEntry) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            if workspace.mode == .folders {
                breadcrumbs
            } else {
                Label(scope.path, systemImage: "externaldrive")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(scope.path)
            }

            HStack(alignment: .bottom, spacing: 17) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(workspace.mode == .folders ? scope.name : workspace.mode.rawValue)
                        .font(.system(size: 28, weight: .semibold))
                        .tracking(-0.6)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .accessibilityAddTraits(.isHeader)
                    Text(scopeCaption(scope))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .layoutPriority(1)

                Spacer(minLength: 4)

                sizeSummary(scope)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.quaternary.opacity(0.34), in: RoundedRectangle(cornerRadius: 9))
                .fixedSize()
            }
        }
        .padding(.horizontal, 24)
        .padding(.top, 14)
        .padding(.bottom, 17)
        .background(alignment: .topTrailing) {
            LinearGradient(
                colors: [SpaceColor.accent.opacity(0.065), .clear],
                startPoint: .topTrailing,
                endPoint: .bottomLeading
            )
            .accessibilityHidden(true)
        }
    }

    private func scopeCaption(_ scope: ScanEntry) -> String {
        let excluded = workspace.report?.excludedCount ?? 0
        var suffix = excluded == 0 ? "" : " · \(ScanFormat.excluded(excluded))"
        if workspace.isAgeFiltered, let days = workspace.olderThanDays {
            suffix += " · files older than \(WorkspaceCommands.ageTitle(days).lowercased())"
        } else if workspace.olderThanDays != nil, workspace.report?.isPreview == true {
            suffix += " · the age filter applies when the scan finishes"
        }
        if let progress = workspace.exclusiveProgress {
            suffix += " · Measuring exclusive sizes \(Int(progress * 100))%"
        } else if workspace.showsExclusive, workspace.importedFile != nil {
            suffix += " · Exclusive sizes need a scan of the files"
        }
        if workspace.mode == .folders {
            if scope.isCloudOnly { return "Cloud only" }
            let count = ScanFormat.items(scope.children.count)
            if scope.isPending { return "\(count) found so far" + suffix }
            return count + suffix
        }
        if workspace.mode == .duplicates {
            guard let groups = workspace.duplicateGroups else { return "Files of 1 MB or more with the same contents" }
            let extra = groups.reduce(UInt64(0)) { $0 + $1.extraBytes }
            let count = groups.count == 1 ? "1 group" : "\(groups.count) groups"
            return "\(count) · \(ScanFormat.size(extra)) in extra copies"
        }
        if workspace.mode == .cleanup {
            if workspace.report?.isPreview == true { return "Suggestions appear when the scan finishes" }
            // Folders to review can't be recreated, so they stay out of the total.
            let total = workspace.rows.reduce(UInt64(0)) { total, entry in
                workspace.reviewPaths.contains(entry.path) ? total : total + entry.bytes(for: workspace.sizeMetric)
            }
            let review = workspace.rows.filter { workspace.reviewPaths.contains($0.path) }.count
            let suffix = review == 0 ? "" : " · \(review) to review"
            return "\(ScanFormat.size(total)) in folders that tools recreate when needed" + suffix
        }
        if workspace.report?.isPreview == true { return "Largest files found so far" + suffix }
        return "Top \(ScanWorkspace.largestFileLimit) files across the scan root" + suffix
    }

    private func sizeSummary(_ scope: ScanEntry) -> some View {
        let bytes = scope.bytes(for: workspace.sizeMetric)
        let word = workspace.sizeMetric.title
        let title = scope.isPending ? "\(word) so far" : word
        return VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(ScanFormat.size(bytes))
                .font(.system(size: 18, weight: .semibold).monospacedDigit())
                .foregroundStyle(SpaceColor.accent)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(ScanFormat.exactSize(bytes))
        .help(ScanFormat.sizeDetail(bytes))
    }

    private var breadcrumbs: some View {
        HStack(spacing: 8) {
            Button(action: workspace.browseParent) {
                Image(systemName: "chevron.up")
                    .font(.caption.weight(.semibold))
                    .frame(width: 16)
            }
            .accessibilityLabel("Go to parent folder")
            .help("Go to the parent folder (⌘↑)")
            .disabled(!workspace.canBrowseParent)

            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    HStack(spacing: 4) {
                        ForEach(Array(workspace.scope.enumerated()), id: \.element.id) { index, entry in
                            if index > 0 {
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 8, weight: .semibold))
                                    .foregroundStyle(.tertiary)
                                    .accessibilityHidden(true)
                            }
                            if entry.id == workspace.currentFolder?.id {
                                Text(entry.name)
                                    .fontWeight(.semibold)
                                    .help(entry.path)
                                    .accessibilityLabel("Current folder, \(entry.name)")
                                    .id(entry.id)
                            } else {
                                Button(entry.name) { workspace.browseBreadcrumb(at: index) }
                                    .buttonStyle(.borderless)
                                    .help(entry.path)
                                    .id(entry.id)
                            }
                        }
                    }
                    .font(.subheadline)
                }
                .scrollIndicators(.hidden)
                .defaultScrollAnchor(.trailing)
                .onChange(of: workspace.currentFolder?.id) { _, path in
                    if let path { proxy.scrollTo(path, anchor: .trailing) }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Folder path")
    }

    /// Real scan errors get a warning. Protected items alone get a quieter notice.
    @ViewBuilder
    private var scanNotice: some View {
        let errors = workspace.errorIssueCount
        let protected = workspace.protectedIssueCount
        if errors > 0 {
            noticeBar(
                symbol: "exclamationmark.triangle.fill",
                isWarning: true,
                message: (errors == 1
                    ? "Partial scan. One scan error affects these totals."
                    : "Partial scan. \(errors.formatted()) scan errors affect these totals.")
                    + (protected == 1 ? " One protected item wasn't counted." : "")
                    + (protected > 1 ? " \(ScanFormat.protectedItems(protected)) weren't counted." : "")
            )
        } else if protected > 0 {
            noticeBar(
                symbol: "lock.fill",
                isWarning: false,
                message: protected == 1
                    ? "One protected item wasn't counted. macOS doesn't let apps read it."
                    : "\(ScanFormat.protectedItems(protected)) weren't counted. macOS doesn't let apps read them."
            )
        }
    }

    private func noticeBar(symbol: String, isWarning: Bool, message: String) -> some View {
        let tint: Color = isWarning ? .orange : .secondary
        return HStack(spacing: 8) {
            Image(systemName: symbol)
                .foregroundStyle(tint)
                .accessibilityHidden(true)
            Text(message)
                .font(.subheadline)
                .foregroundStyle(isWarning ? .primary : .secondary)
            Spacer()
            switch workspace.accessAdvice {
            case .grant:
                Button("Allow Full Disk Access…", action: FullDiskAccess.openSettings)
                    .buttonStyle(.borderless)
                    .help("Open Privacy & Security settings to let Space 16X read privacy-protected folders")
            case .rescan:
                Button("Scan Again", action: workspace.rescan)
                    .buttonStyle(.borderless)
                    .disabled(!workspace.canRescan)
                    .help("Full Disk Access is on. Scan again to include the folders it unlocks.")
            case .none:
                EmptyView()
            }
            Button("View Details") { workspace.showsDetails = true }
                .buttonStyle(.borderless)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 9)
        .background(tint.opacity(isWarning ? 0.11 : 0.08))
    }

    private var presentationBar: some View {
        HStack(spacing: 8) {
            Text("Contents")
                .font(.subheadline.weight(.semibold))
                .accessibilityHidden(true)
            Picker("Folder view", selection: $workspace.folderPresentation) {
                ForEach(FolderPresentation.available(pro: proFeatures), id: \.self) { presentation in
                    Text(presentation.rawValue).tag(presentation)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: proFeatures ? 280 : 210)
            Spacer(minLength: 0)
            if proFeatures {
                compareControl
            }
        }
        .tint(Color(nsColor: .controlAccentColor))
        .padding(.horizontal, 24)
        .padding(.vertical, 8)
    }

    /// Pro: starts a comparison, or summarizes the open folder's change and stops it.
    @ViewBuilder
    private var compareControl: some View {
        if let comparison = workspace.comparison {
            let changes = workspace.folderChanges
            HStack(spacing: 6) {
                Image(systemName: "arrow.left.arrow.right")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(changes?.total.map { "\(ScanFormat.change($0)) since “\(comparison.name)”" }
                    ?? "New since “\(comparison.name)”")
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let removed = changes?.removed, !removed.isEmpty {
                    Text("· \(ScanFormat.items(removed.count)) removed")
                        .foregroundStyle(.secondary)
                        .help(removed.prefix(12).map(\.name).joined(separator: "\n"))
                }
                Button { workspace.showsReceiptSheet = true } label: {
                    Image(systemName: "doc.text")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("Save a Build Footprint receipt of these changes")
                .accessibilityLabel("Save Footprint Receipt")
                .disabled(!workspace.canSaveReceipt)
                Button(action: workspace.stopComparing) {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("Stop comparing")
                .accessibilityLabel("Stop comparing")
            }
            .font(.subheadline)
        } else {
            Menu {
                if !workspace.savedScans.isEmpty {
                    Section("History") {
                        ForEach(workspace.savedScans.prefix(12)) { record in
                            Button(historyLabel(record)) { workspace.compare(with: record) }
                        }
                    }
                }
                Button("Choose a Saved Scan…", action: workspace.compareWithSavedScan)
                Divider()
                Button("Save to History", action: workspace.saveToHistory)
                    .disabled(!workspace.canSaveToHistory)
            } label: {
                Label("Compare…", systemImage: "arrow.left.arrow.right")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .font(.subheadline)
            .disabled(!workspace.canCompare)
            .help("Compare with a saved scan to see what grew or shrank, or save this one (Pro)")
        }
    }

    private func historyLabel(_ record: SavedScanRecord) -> String {
        let date = record.date.formatted(date: .abbreviated, time: .shortened)
        return "\(date) · \(ScanFormat.size(record.allocatedBytes))" + (record.isPartial ? " · partial" : "")
    }

    @ViewBuilder
    private func exclusiveCell(_ entry: ScanEntry) -> some View {
        if let sizes = workspace.exclusiveSizes {
            let bytes = sizes.bytes(for: entry)
            Text(ScanFormat.size(bytes))
                .monospacedDigit()
                .foregroundStyle(bytes < entry.allocatedBytes ? Color.secondary : Color.primary)
                .help(bytes < entry.allocatedBytes
                    ? "Emptying the Trash frees at least this much: clones or other hard links share the rest"
                    : "Emptying the Trash frees all of it")
        } else {
            Text("…")
                .foregroundStyle(.tertiary)
                .accessibilityLabel(workspace.importedFile == nil ? "Measuring" : "Not measured for opened scans")
        }
    }

    @ViewBuilder
    private func changeCell(_ entry: ScanEntry) -> some View {
        switch workspace.folderChanges?.change(for: entry) {
        case .added:
            Text("New")
                .foregroundStyle(.orange)
                .help("Not in the saved scan")
        case let .changed(delta) where delta != 0:
            Text(ScanFormat.change(delta))
                .monospacedDigit()
                .foregroundStyle(delta > 0 ? Color.orange : SpaceColor.accent)
                .help(delta > 0 ? "Grew since the saved scan" : "Shrank since the saved scan")
        default:
            Text("—")
                .foregroundStyle(.tertiary)
                .accessibilityLabel("No change")
        }
    }

    private func resultsTable(scope: ScanEntry) -> some View {
        Table(
            workspace.rows, selection: $workspace.selectedPaths, sortOrder: $workspace.sortOrder,
            columnCustomization: $columns
        ) {
            TableColumn("Name", sortUsing: EntrySort(key: .name, order: .forward)) { entry in
                nameCell(entry, scope: scope)
            }
            .width(min: 190, ideal: 280)

            TableColumn(workspace.sizeMetric.title, sortUsing: EntrySort.largestFirst) { entry in
                sizeCell(entry)
            }
            .width(min: 88, ideal: 108, max: 164)

            TableColumn("Items", sortUsing: EntrySort(key: .items, order: .reverse)) { entry in
                itemsCell(entry)
            }
            .width(min: 52, ideal: 72, max: 110)

            // Pro: what moving each item to the Trash and emptying it frees at least.
            TableColumn("Exclusive", sortUsing: EntrySort.mostExclusive) { entry in
                exclusiveCell(entry)
            }
            .width(min: 80, ideal: 100, max: 150)
            .customizationID("exclusive")
            .defaultVisibility(.hidden)
            .disabledCustomizationBehavior(.visibility)

            TableColumn("Change", sortUsing: EntrySort.biggestGrowth) { entry in
                changeCell(entry)
            }
            .width(min: 80, ideal: 100, max: 150)
            .customizationID("change")
            .defaultVisibility(.hidden)
            .disabledCustomizationBehavior(.visibility)

            // Pro: each item's newest modification time, newest first when sorted.
            TableColumn("Modified", sortUsing: EntrySort(key: .modified, order: .reverse)) { entry in
                modifiedCell(entry)
            }
            .width(min: 84, ideal: 100, max: 150)
            .customizationID("modified")
            .defaultVisibility(.hidden)
            .disabledCustomizationBehavior(.visibility)

            TableColumn("Share") { entry in
                shareCell(entry, of: scope)
            }
            .width(min: 85, ideal: 118, max: 155)

        }
        .tableStyle(.inset(alternatesRowBackgrounds: false))
        // Rebuild the table when a Pro column appears or leaves, so its cells fill in.
        .id([showsChanges, proFeatures, showsExclusive])
        .contextMenu(forSelectionType: String.self) { identifiers in
            if identifiers.count > 1 {
                Button("Move \(identifiers.count) Items to Trash…") {
                    workspace.requestTrash(workspace.rows.filter { identifiers.contains($0.path) })
                }
                .disabled(workspace.isScanning)
            } else if let entry = entry(for: identifiers) {
                rowActions(entry)
            }
        } primaryAction: { identifiers in
            if let entry = entry(for: identifiers) { workspace.activate(entry) }
        }
        .onKeyPress(.return) {
            guard let entry = workspace.selectedEntry else { return .ignored }
            workspace.activate(entry)
            return .handled
        }
        .onKeyPress(.space) {
            guard workspace.canQuickLook || workspace.quickLookURL != nil else { return .ignored }
            workspace.toggleQuickLook()
            return .handled
        }
        .quickLookPreview($workspace.quickLookURL)
        .accessibilityLabel(workspace.mode == .folders ? "Folder contents" : workspace.mode.rawValue)
        .overlay {
            if workspace.mode == .duplicates && workspace.rows.isEmpty {
                duplicatesPlaceholder
            } else if workspace.isLoadingRows && workspace.rows.isEmpty {
                ProgressView("Ranking items…")
                    .controlSize(.small)
                    .padding(16)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            } else if workspace.rows.isEmpty {
                VStack(spacing: 9) {
                    Image(systemName: workspace.mode.symbol)
                        .font(.title)
                        .foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                    Text(emptyMessage(for: scope))
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    /// Pro: Duplicates reads files only after the user clicks Find Duplicates.
    @ViewBuilder
    private var duplicatesPlaceholder: some View {
        if workspace.isFindingDuplicates {
            ProgressView("Comparing files…")
                .controlSize(.small)
                .padding(16)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
        } else {
            VStack(spacing: 10) {
                Image(systemName: BrowseMode.duplicates.symbol)
                    .font(.title)
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
                if workspace.duplicateGroups == nil {
                    Text("Find files with the same contents")
                        .font(.headline)
                    Text("Space 16X reads files of 1 MB or more that share a length with another file and compares their contents. Nothing is read until you start.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 380)
                    Button("Find Duplicates", action: workspace.findDuplicates)
                        .controlSize(.large)
                        .disabled(workspace.isScanning || workspace.report?.isPreview != false)
                } else {
                    Text("No duplicate files of 1 MB or more")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(24)
        }
    }

    private func emptyMessage(for scope: ScanEntry) -> String {
        if scope.isCloudOnly { return "Contents are available in the cloud" }
        if workspace.protectedPaths.contains(scope.path) { return "macOS doesn't allow apps to read this folder" }
        if workspace.issuePaths.contains(scope.path) { return "Folder metadata is unavailable" }
        if scope.isPending {
            return workspace.isScanning ? "Waiting for scanned items…" : "Contents were not scanned"
        }
        switch workspace.mode {
        case .folders: return "No scanned items here"
        case .largestFiles: return "No files to rank"
        case .cleanup: return "No node_modules, build output or caches of 1 MB or more"
        case .duplicates: return "No duplicate files of 1 MB or more"
        }
    }

    @ViewBuilder
    private func entryStatus(_ entry: ScanEntry) -> some View {
        if entry.isCloudOnly {
            Label("Cloud only", systemImage: "cloud")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if workspace.protectedPaths.contains(entry.path) {
            Label("Protected", systemImage: "lock")
                .font(.caption)
                .foregroundStyle(.secondary)
                .help("macOS doesn't let apps read this item, so its contents aren't counted")
        } else if workspace.issuePaths.contains(entry.path) {
            Label("Incomplete", systemImage: "exclamationmark.triangle")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if entry.isPending {
            let currentPath = workspace.progress?.currentPath ?? ""
            let isActive = workspace.isScanning && (currentPath == entry.path || currentPath.hasPrefix(entry.path + "/"))
            Label(
                isActive ? "Scanning…" : (workspace.isScanning ? "Pending" : "Unscanned"),
                systemImage: isActive ? "arrow.clockwise" : "clock"
            )
            .font(.caption)
            .foregroundStyle(isActive ? SpaceColor.accent : .secondary)
        }
    }

    private func nameCell(_ entry: ScanEntry, scope: ScanEntry) -> some View {
        HStack(spacing: 8) {
            Image(systemName: entry.isDirectory ? "folder.fill" : "doc")
                .foregroundStyle(entry.isDirectory ? SpaceColor.accent : .secondary)
                .frame(width: 16)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if workspace.mode == .cleanup || workspace.mode == .duplicates,
                   let label = workspace.rowLabels[entry.path] {
                    Text([label, relativeParent(of: entry, to: scope)].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                } else if workspace.mode == .largestFiles, let parent = relativeParent(of: entry, to: scope) {
                    Text(parent)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            if entry.isHardLinkAlias {
                Image(systemName: "link")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .help("Hard link counted elsewhere in this scan")
                    .accessibilityLabel("Hard link counted elsewhere")
            }
            entryStatus(entry)
                .fixedSize()
        }
        .help(entry.path)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(entry.isDirectory ? .isButton : [])
    }

    private func relativeParent(of entry: ScanEntry, to scope: ScanEntry) -> String? {
        let prefix = scope.path.hasSuffix("/") ? scope.path : scope.path + "/"
        let path = entry.path.hasPrefix(prefix) ? String(entry.path.dropFirst(prefix.count)) : entry.path
        guard let separator = path.lastIndex(of: "/") else { return nil }
        return String(path[..<separator])
    }

    @ViewBuilder
    private func modifiedCell(_ entry: ScanEntry) -> some View {
        if let date = entry.newestModification {
            Text(date, format: .dateTime.year().month(.abbreviated).day())
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .help(entry.isDirectory
                    ? "The newest modification time of any file inside"
                    : "Last modified \(date.formatted(date: .long, time: .shortened))")
        } else {
            Text("—")
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .accessibilityLabel("No modification time")
        }
    }

    @ViewBuilder
    private func itemsCell(_ entry: ScanEntry) -> some View {
        if entry.isDirectory && !entry.isCloudOnly {
            let unknown = entry.isPending || workspace.issuePaths.contains(entry.path)
            Text(unknown && entry.itemCount == 0 ? "—" : entry.itemCount.formatted())
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .help("Files and folders inside, counting each hard-linked file once")
                .accessibilityLabel(unknown && entry.itemCount == 0 ? "Items unknown" : ScanFormat.items(entry.itemCount))
        } else {
            Text("")
        }
    }

    private func sizeCell(_ entry: ScanEntry) -> some View {
        let bytes = entry.bytes(for: workspace.sizeMetric)
        let unknown = bytes == 0 && (entry.isPending || workspace.issuePaths.contains(entry.path))
        let exact = unknown
            ? (workspace.isScanning ? "Size pending" : "Size unknown")
            : ScanFormat.exactSize(bytes)
        return Text(unknown ? "—" : ScanFormat.size(bytes))
            .monospacedDigit()
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .trailing)
            .foregroundStyle(unknown ? .secondary : .primary)
            .help(unknown ? exact : ScanFormat.sizeDetail(bytes))
            .accessibilityLabel(exact)
    }

    @ViewBuilder
    private func shareCell(_ entry: ScanEntry, of scope: ScanEntry) -> some View {
        let bytes = entry.bytes(for: workspace.sizeMetric)
        if bytes == 0 && (entry.isPending || workspace.issuePaths.contains(entry.path)) {
            Text("—")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .accessibilityLabel("Share unknown")
        } else {
            knownShare(entry, of: scope)
        }
    }

    private func knownShare(_ entry: ScanEntry, of scope: ScanEntry) -> some View {
        let total = scope.bytes(for: workspace.sizeMetric)
        let ratio = total == 0 ? 0 : min(Double(entry.bytes(for: workspace.sizeMetric)) / Double(total), 1)
        let formatted = ratio.formatted(.percent.precision(.fractionLength(0...1)))
        return HStack(spacing: 5) {
            GeometryReader { proxy in
                Capsule()
                    .fill(.tertiary.opacity(0.35))
                    .overlay(alignment: .leading) {
                        Capsule()
                            .fill(SpaceColor.accent.opacity(0.84))
                            .frame(width: proxy.size.width * min(ratio, 1))
                    }
            }
            .frame(maxWidth: 62)
            .frame(height: 4)
            Text(formatted)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 44, alignment: .trailing)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(formatted) of counted \(workspace.sizeMetric.title.lowercased()) bytes")
    }

    private var selectionBar: some View {
        HStack(spacing: 12) {
            if workspace.selectedPaths.count > 1 {
                let selected = workspace.selectedEntries
                let bytes = selected.reduce(UInt64(0)) { $0 + $1.bytes(for: workspace.sizeMetric) }
                Text("\(selected.count) items selected · \(ScanFormat.size(bytes))")
            } else if let entry = workspace.selectedEntry {
                Text(entry.path)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .help(entry.path)
                    .accessibilityLabel("Selected path, \(entry.path)")
            } else {
                Text(ScanFormat.items(workspace.rows.count))
            }
            Spacer(minLength: 0)
            if workspace.selectedEntry?.isDirectory == true {
                Button(action: workspace.openSelectedFolder) {
                    Label("Open Folder", systemImage: "folder")
                }
                .help("Browse the selected folder (⌘↓)")
            }
            Button(action: workspace.revealSelected) {
                Label("Reveal in Finder", systemImage: "arrow.up.forward.app")
            }
            .help("Reveal the selected item in Finder (⇧⌘R)")
            .disabled(workspace.selectedEntry == nil || !workspace.canReveal)
            Button(action: workspace.copySelectedPath) {
                Label("Copy Path", systemImage: "doc.on.doc")
            }
            .help("Copy the selected path (⌥⌘C)")
            .disabled(workspace.selectedEntry == nil)
            Button { workspace.requestTrash() } label: {
                Label("Move to Trash", systemImage: "trash")
            }
            .help("Move the selected items to the Trash after you confirm (⌘⌫)")
            .disabled(!workspace.canMoveToTrash)
        }
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .buttonStyle(.borderless)
        .padding(.horizontal, 24)
        .padding(.vertical, 10)
        .background(.bar)
    }

    private func entry(for identifiers: Set<String>) -> ScanEntry? {
        guard let identifier = identifiers.first else { return nil }
        return workspace.entry(withPath: identifier)
    }

    @ViewBuilder
    private func rowActions(_ entry: ScanEntry) -> some View {
        if entry.isDirectory {
            Button("Open Folder") { workspace.browse(entry) }
            Button("Rescan Folder") { workspace.rescanFolder(entry) }
                .disabled(!workspace.canRescan)
            Divider()
        }
        Button("Quick Look") {
            workspace.selection = entry.path
            workspace.toggleQuickLook()
        }
        .disabled(workspace.isImported)
        Button("Reveal in Finder") { workspace.reveal(entry) }
            .disabled(workspace.isImported)
        Button("Copy Path") { workspace.copyPath(of: entry) }
        Divider()
        Button("Move to Trash…") { workspace.requestTrash([entry]) }
            .disabled(workspace.isScanning || workspace.isImported)
    }
}
