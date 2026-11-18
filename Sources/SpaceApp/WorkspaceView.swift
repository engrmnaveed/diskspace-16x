import SpaceCore
import SwiftUI

struct WorkspaceView: View {
    @Bindable var workspace: ScanWorkspace
    @ViewState private var isDropTargeted = false

    var body: some View {
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 180, ideal: 216, max: 265)
        } detail: {
            VStack(spacing: 0) {
                if let notice = workspace.notice {
                    noticeBar(notice)
                }
                if workspace.isScanning {
                    scanInProgress
                } else if workspace.report != nil {
                    ResultsView(workspace: workspace)
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
                ScanDetailsView(report: report)
            }
        }
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

                ForEach(BrowseMode.allCases, id: \.self) { mode in
                    modeButton(mode)
                }
            }

            Spacer(minLength: 16)

            if let report = workspace.report {
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
            }
            .padding(.horizontal, 7)

            Button {
                workspace.showsDetails = true
            } label: {
                if report.isPartial {
                    Label("Partial scan", systemImage: "exclamationmark.triangle")
                        .symbolRenderingMode(.hierarchical)
                } else {
                    Label("Scan Details", systemImage: "info.circle")
                }
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 7)
            .help("Review the scan scope and any skipped paths (⌘I)")
            .disabled(workspace.isScanning)
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

    private var scanInProgress: some View {
        VStack(spacing: 17) {
            SpaceMark(size: 54)
            VStack(spacing: 8) {
                Text(workspace.isCancelling ? "Cancelling scan…" : "Scanning metadata…")
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
