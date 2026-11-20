import AppKit
import SpaceCore
import SwiftUI

struct ScanDetailsView: View {
    let report: ScanReport
    let workspace: ScanWorkspace
    @Environment(\.dismiss) private var dismiss
    private let errors: [ScanIssue]
    private let protectedGroups: [IssueGroup]
    private let protectedCount: Int
    private let hasPrivacyRefusals: Bool

    init(report: ScanReport, workspace: ScanWorkspace) {
        self.report = report
        self.workspace = workspace
        errors = report.errorIssues
        let protected = report.protectedIssues
        protectedCount = protected.count
        protectedGroups = IssueGroup.grouping(protected, under: report.root.path)
        hasPrivacyRefusals = protected.contains { $0.kind == .notPermitted }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 5) {
                Label(
                    errors.isEmpty ? "Scan Details" : "Partial Scan",
                    systemImage: errors.isEmpty ? "info.circle" : "exclamationmark.triangle"
                )
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
                Text(report.root.path)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(3)
                    .truncationMode(.middle)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("How sizes are counted")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                Text("Folder totals sum regular files in the scan. Space 16X skips symbolic links and other devices, and counts each hard-linked file once.")
                Text("Allocated size is what the filesystem reports. APFS clones can share blocks and snapshots can retain them, so totals can differ from volume usage or from the space recovered after deletion.")
                Text("Cloud-only entries are shown as references. They contribute no bytes, and cloud-only folders stay unexpanded.")
                if report.isPreview {
                    Text("This preview shows allocated space counted so far. Folders awaiting a deeper scan have a status in the list.")
                }
            }
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

            if report.issues.isEmpty {
                Label("No scan errors", systemImage: "checkmark.circle")
                    .foregroundStyle(SpaceColor.accent)
            } else {
                skippedItems
            }

            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 620)
    }

    private var skippedItems: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Skipped items")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            if !errors.isEmpty {
                Text("Scan errors make the totals incomplete.")
                    .foregroundStyle(.secondary)
            }
            if protectedCount > 0 {
                protectedExplanation
            }
            List {
                if !errors.isEmpty {
                    Section("Scan errors (\(errors.count.formatted()))") {
                        ForEach(errors) { issue in
                            VStack(alignment: .leading, spacing: 3) {
                                Text(issue.path)
                                    .textSelection(.enabled)
                                Text(issue.message)
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                            }
                            .padding(.vertical, 4)
                        }
                    }
                }
                if protectedCount > 0 {
                    Section("Protected items (\(protectedCount.formatted()))") {
                        ForEach(protectedGroups) { group in
                            if group.issues.count == 1, let issue = group.issues.first {
                                protectedRow(issue, title: Self.displayPath(issue.path))
                            } else {
                                DisclosureGroup {
                                    ForEach(group.issues) { issue in
                                        protectedRow(issue, title: group.relativePath(of: issue))
                                    }
                                } label: {
                                    HStack {
                                        Text(group.displayPath)
                                            .lineLimit(1)
                                            .truncationMode(.middle)
                                        Spacer()
                                        Text(group.issues.count.formatted())
                                            .monospacedDigit()
                                            .foregroundStyle(.secondary)
                                    }
                                    .help(group.folder)
                                    .accessibilityElement(children: .combine)
                                }
                            }
                        }
                    }
                }
            }
            .listStyle(.inset(alternatesRowBackgrounds: false))
            .frame(minHeight: 140, idealHeight: 260)
            .overlay {
                RoundedRectangle(cornerRadius: 5)
                    .strokeBorder(.quaternary)
                    .allowsHitTesting(false)
            }
            .accessibilityLabel("Skipped items")
        }
    }

    private var protectedExplanation: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("macOS doesn't let apps read protected items, so their contents aren't in the totals. This is expected when you scan system locations.")
            switch workspace.accessAdvice {
            case .grant:
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("Full Disk Access lets Space 16X read folders that privacy settings protect, such as other apps' data. Turn it on, then scan again. If items are still skipped, quit and reopen Space 16X.")
                    Spacer(minLength: 0)
                    Button("Allow Full Disk Access…", action: FullDiskAccess.openSettings)
                }
            case .rescan:
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("Full Disk Access is on. Scan again to include the folders it unlocks.")
                    Spacer(minLength: 0)
                    Button("Scan Again", action: workspace.rescan)
                        .disabled(!workspace.canRescan)
                }
            case .none:
                Text(hasPrivacyRefusals
                    ? "Full Disk Access is on. The remaining items are readable only by macOS itself."
                    : "Their permissions let only their owner, usually macOS, read them.")
            }
        }
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }

    static func displayPath(_ path: String) -> String {
        (path as NSString).abbreviatingWithTildeInPath
    }

    private func protectedRow(_ issue: ScanIssue, title: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .lineLimit(1)
                .truncationMode(.middle)
            Text(issue.kind == .permissionDenied
                ? "Readable only by its owner"
                : "Blocked by macOS privacy or system protection")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
        .help("\(issue.path)\n\(issue.message)")
        .contextMenu {
            Button("Copy Path") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(issue.path, forType: .string)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private extension IssueGroup {
    var displayPath: String { ScanDetailsView.displayPath(folder) }
}
