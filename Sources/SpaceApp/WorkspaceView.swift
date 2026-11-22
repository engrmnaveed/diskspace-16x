import AppKit
import SpaceCore
import SpacePro
import SwiftUI

struct WorkspaceView: View {
    @Bindable var workspace: ScanWorkspace
    @ViewState private var isDropTargeted = false
    @AppStorage(ProFeatures.defaultsKey) private var proFeatures = false
    @AppStorage(ProFeatures.hintDismissedKey) private var proHintDismissed = false

    var body: some View {
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 180, ideal: 216, max: 265)
        } detail: {
            VStack(spacing: 0) {
                if let notice = workspace.notice {
                    noticeBar(notice)
                }
                if let file = workspace.openingFile {
                    opening(file)
                } else if workspace.canBrowse {
                    if workspace.isScanning { backgroundScanProgress }
                    ResultsView(workspace: workspace)
                } else if workspace.isScanning {
                    scanInProgress
                } else {
                    startView
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(SpaceColor.canvas)
            .overlay {
                if isDropTargeted && workspace.canChooseFolder {
                    RoundedRectangle(cornerRadius: 12)
                        .fill(SpaceColor.accent.opacity(0.07))
                        .strokeBorder(SpaceColor.accent, style: StrokeStyle(lineWidth: 2, dash: [6, 5]))
                        .padding(9)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
            }
            .dropDestination(for: URL.self) { urls, _ in
                workspace.acceptDrop(urls)
            } isTargeted: { isDropTargeted = $0 }
        }
        .tint(SpaceColor.accent)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button(action: workspace.rescan) {
                    Label("Scan Again", systemImage: "arrow.clockwise")
                }
                .help("Scan the same location again (⌘R)")
                .disabled(!workspace.canRescan)

                Button(action: workspace.chooseFolder) {
                    Label("Choose Folder…", systemImage: "folder.badge.plus")
                }
                .help("Choose a folder or drive to scan (⌘O)")
                .disabled(!workspace.canChooseFolder)
            }
        }
        .sheet(isPresented: $workspace.showsDetails) {
            if let report = workspace.report {
                ScanDetailsView(report: report, workspace: workspace)
            }
        }
        .sheet(item: $workspace.explanation) { explanation in
            ExplainDiskView(explanation: explanation)
        }
        .sheet(isPresented: $workspace.showsReceiptSheet) {
            ReceiptSheet(workspace: workspace)
        }
        // Returning from System Settings may have changed Full Disk Access.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            workspace.refreshFullDiskAccess()
            workspace.refreshVolumes()
        }
        .onReceive(NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didMountNotification)) { _ in
            workspace.refreshVolumes()
        }
        .onReceive(NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didUnmountNotification)) { _ in
            workspace.refreshVolumes()
        }
        .task { workspace.refreshVolumes() }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 9) {
                SpaceMark(size: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Space 16X")
                        .font(.headline)
                    Text("16X Brains")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 7)
            .accessibilityElement(children: .combine)

            VStack(alignment: .leading, spacing: 3) {
                Text("Explore")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 4)
                    .accessibilityAddTraits(.isHeader)

                ForEach(BrowseMode.available(pro: proFeatures), id: \.self) { mode in
                    modeButton(mode)
                }
            }

            Spacer(minLength: 16)

            if workspace.canBrowse, let report = workspace.report {
                sidebarSummary(report)
            }
        }
        .padding(12)
        .frame(maxHeight: .infinity, alignment: .top)
    }

    private func modeButton(_ mode: BrowseMode) -> some View {
        let isSelected = workspace.report != nil && workspace.mode == mode
        return Button {
            workspace.mode = mode
        } label: {
            HStack(spacing: 9) {
                Image(systemName: mode.symbol)
                    .frame(width: 17)
                    .foregroundStyle(isSelected ? SpaceColor.accent : .secondary)
                Text(mode.rawValue)
                    .fontWeight(isSelected ? .semibold : .regular)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? SpaceColor.accent.opacity(0.13) : .clear,
                in: RoundedRectangle(cornerRadius: 7)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .disabled(!workspace.canBrowse)
    }

    private func sidebarSummary(_ report: ScanReport) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Divider()
            VStack(alignment: .leading, spacing: 4) {
                Text("Scan root")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .accessibilityAddTraits(.isHeader)
                Label(report.root.name, systemImage: "externaldrive")
                    .fontWeight(.medium)
                    .lineLimit(1)
                Text(report.root.path)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(report.root.path)
                if let file = workspace.importedFile {
                    Label("Saved scan, read-only", systemImage: "doc.text")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.top, 2)
                        .help("Opened from \(file). Scan the folder again to rescan it or move items to the Trash.")
                }
            }
            .padding(.horizontal, 7)

            Button {
                workspace.showsDetails = true
            } label: {
                if workspace.errorIssueCount > 0 {
                    Label("Partial scan", systemImage: "exclamationmark.triangle")
                        .symbolRenderingMode(.hierarchical)
                } else if report.isPartial {
                    Label("Protected items skipped", systemImage: "lock")
                        .foregroundStyle(.secondary)
                } else {
                    Label("Scan Details", systemImage: "info.circle")
                }
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 7)
            .help("Review the scan scope and any skipped paths (⌘I)")
        }
    }

    private var startView: some View {
        VStack(spacing: 22) {
            SpaceMark(size: 80)
                .shadow(color: .black.opacity(0.16), radius: 18, y: 8)

            VStack(spacing: 8) {
                Text("Find what’s using space")
                    .font(.system(size: 29, weight: .semibold))
                    .tracking(-0.65)
                    .accessibilityAddTraits(.isHeader)
                Text("Choose a local folder or drive, or drop one here from Finder.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
            }

            Button(action: workspace.chooseFolder) {
                Label("Choose Folder…", systemImage: "folder")
                    .padding(.horizontal, 7)
                    .foregroundStyle(workspace.canChooseFolder ? SpaceColor.onAccent : Color.secondary)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!workspace.canChooseFolder)
            .help("Open the folder picker (⌘O)")

            Button("Open a Saved Scan…", action: workspace.openScan)
                .buttonStyle(.link)
                .disabled(!workspace.canChooseFolder)
                .help("Browse a scan exported by Space 16X, the CLI, ncdu, or gdu (⇧⌘O)")

            if !workspace.volumes.isEmpty {
                volumeList
            }

            if !proFeatures && !proHintDismissed {
                proHint
            }
        }
        .padding(36)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            RadialGradient(
                colors: [SpaceColor.accent.opacity(0.075), .clear],
                center: .center,
                startRadius: 10,
                endRadius: 310
            )
        }
    }

    /// One quiet line, while Pro features are off, that opens Settings, where the switch is.
    private var proHint: some View {
        HStack(spacing: 6) {
            Image(systemName: "sparkles")
                .foregroundStyle(SpaceColor.accent)
                .accessibilityHidden(true)
            Text("More views and a menu-bar monitor are free in Pro.")
                .foregroundStyle(.secondary)
            SettingsLink {
                Text("Turn On in Settings…")
            }
            .buttonStyle(.link)
            .help("Open Settings → Pro features")
            Button {
                proHintDismissed = true
            } label: {
                Image(systemName: "xmark")
                    .font(.caption2.weight(.semibold))
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.tertiary)
            .help("Hide this hint")
            .accessibilityLabel("Hide the Pro hint")
        }
        .font(.callout)
        .padding(.top, 4)
    }

    private var volumeList: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text("Volumes")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 4)
                .accessibilityAddTraits(.isHeader)
            ForEach(workspace.volumes) { volume in
                volumeButton(volume)
            }
        }
        .frame(maxWidth: 400)
        .padding(.top, 6)
    }

    private func volumeButton(_ volume: VolumeSummary) -> some View {
        let free = "\(ScanFormat.size(volume.availableBytes)) free of \(ScanFormat.size(volume.totalBytes))"
        return Button {
            workspace.startScan(URL(filePath: volume.path, directoryHint: .isDirectory))
        } label: {
            HStack(spacing: 11) {
                Image(systemName: volume.path == "/" ? "internaldrive" : "externaldrive")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(volume.name)
                            .fontWeight(.medium)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(free)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    GeometryReader { proxy in
                        ZStack(alignment: .leading) {
                            Capsule().fill(Color.secondary.opacity(0.18))
                            Capsule()
                                .fill(volume.usedFraction > 0.9 ? Color.orange : SpaceColor.accent)
                                .frame(width: proxy.size.width * volume.usedFraction)
                        }
                    }
                    .frame(height: 5)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(Color.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 9))
            .contentShape(RoundedRectangle(cornerRadius: 9))
        }
        .buttonStyle(.plain)
        .disabled(!workspace.canChooseFolder)
        .contextMenu {
            if proFeatures {
                Button("Explain Disk Space") { workspace.explainDisk(at: volume.path) }
            }
        }
        .help("Scan \(volume.name) (\(volume.path))")
        .accessibilityLabel("\(volume.name), \(free)")
        .accessibilityHint("Scans this volume")
    }

    private func opening(_ file: String) -> some View {
        VStack(spacing: 17) {
            SpaceMark(size: 54)
            Text("Opening “\(file)”…")
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            ProgressView()
                .progressViewStyle(.linear)
                .frame(maxWidth: 260)
                .accessibilityLabel("Opening the saved scan")
        }
        .padding(36)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var scanInProgress: some View {
        VStack(spacing: 17) {
            SpaceMark(size: 54)
            VStack(spacing: 8) {
                Text(workspace.isCancelling ? "Cancelling scan…" : "Scanning the first two levels…")
                    .font(.title2.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                Text(workspace.scanPath ?? "")
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .foregroundStyle(.secondary)
                    .help(workspace.scanPath ?? "")
            }
            ProgressView()
                .progressViewStyle(.linear)
                .frame(maxWidth: 260)
                .accessibilityLabel("Scan in progress")
            if let progress = workspace.progress {
                VStack(spacing: 5) {
                    Text("\(progress.visitedEntries.formatted()) entries visited")
                        .font(.subheadline.monospacedDigit())
                    Text(progress.currentPath)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .accessibilityLabel("Current path, \(progress.currentPath)")
                }
            }

            Button("Cancel Scan", action: workspace.cancelScan)
                .disabled(workspace.isCancelling)
                .keyboardShortcut(.cancelAction)
        }
        .padding(36)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var backgroundScanProgress: some View {
        HStack(spacing: 12) {
            ProgressView()
                .controlSize(.small)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(workspace.isCancelling
                    ? "Cancelling scan…"
                    : workspace.rescanningFolder.map { "Rescanning “\($0)”…" } ?? "Scanning deeper folders…")
                    .font(.subheadline.weight(.semibold))
                Text(workspace.progress?.currentPath ?? workspace.scanPath ?? "")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(workspace.progress?.currentPath ?? workspace.scanPath ?? "")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let progress = workspace.progress {
                Text("\(progress.visitedEntries.formatted()) visited")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .fixedSize()
            }
            Button("Cancel Scan", action: workspace.cancelScan)
                .disabled(workspace.isCancelling)
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 9)
        .background(SpaceColor.accent.opacity(0.08))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Background scan progress")
    }

    private func noticeBar(_ notice: ScanWorkspace.Notice) -> some View {
        HStack(alignment: .top, spacing: 11) {
            Image(systemName: notice.isError ? "exclamationmark.triangle.fill" : "info.circle")
                .foregroundStyle(notice.isError ? .orange : SpaceColor.accent)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(notice.title).fontWeight(.semibold)
                Text(notice.message)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                    .textSelection(.enabled)
            }
            Spacer()
            Button(action: workspace.dismissNotice) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Dismiss message")
        }
        .padding(14)
        .background(notice.isError ? Color.orange.opacity(0.10) : SpaceColor.accent.opacity(0.07))
    }
}
