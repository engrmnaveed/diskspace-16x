import SpaceCore
import SwiftUI

struct ScanDetailsView: View {
    let report: ScanReport
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 5) {
                Label(
                    report.isPartial ? "Partial Scan" : "Scan Details",
                    systemImage: report.isPartial ? "exclamationmark.triangle" : "info.circle"
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
                Text("Logical size is the file length. Allocated size is what the filesystem reports. APFS clones can share blocks and snapshots can retain them, so totals can differ from volume usage or from the space recovered after deletion.")
            }
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

            if report.isPartial {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Skipped paths (\(report.issues.count.formatted()))")
                        .font(.headline)
                        .accessibilityAddTraits(.isHeader)
                    Text("These errors make the totals incomplete.")
                        .foregroundStyle(.secondary)
                    List(report.issues) { issue in
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
                    .listStyle(.inset(alternatesRowBackgrounds: false))
                    .frame(minHeight: 120, idealHeight: 210)
                    .overlay {
                        RoundedRectangle(cornerRadius: 5)
                            .strokeBorder(.quaternary)
                            .allowsHitTesting(false)
                    }
                    .accessibilityLabel("Skipped paths")
                }
            } else {
                Label("No scan errors", systemImage: "checkmark.circle")
                    .foregroundStyle(SpaceColor.accent)
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
}
