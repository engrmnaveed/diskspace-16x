import SpacePro
import SwiftUI

/// Pro: the space on a volume that a scan can't show, as `space16x explain` lists it.
struct ExplainDiskView: View {
    let explanation: DiskExplanation
    @Environment(\.dismiss) private var dismiss

    private var sharesContainer: Bool { explanation.containerVolumes.count > 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 5) {
                Label("Disk Space on \(explanation.name)", systemImage: "internaldrive")
                    .font(.title2.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                Text("\(explanation.mountPoint) · \(explanation.fileSystem.uppercased()) · \(explanation.device)")
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }

            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 14, verticalSpacing: 7) {
                row("Size", ScanFormat.size(explanation.totalBytes))
                row(
                    "Used", ScanFormat.size(explanation.usedBytes),
                    sharesContainer ? "Shared by the volumes of its APFS container, listed below." : nil
                )
                row("Free", ScanFormat.size(explanation.availableBytes))
                if let purgeable = explanation.purgeableBytes {
                    row(
                        "Purgeable", ScanFormat.size(purgeable),
                        "Caches, iCloud copies and snapshots that macOS frees when an app needs the space."
                    )
                }
                row("Snapshots", snapshotSummary, snapshotNote)
            }

            if sharesContainer {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Volumes in this container")
                        .font(.headline)
                        .accessibilityAddTraits(.isHeader)
                    ForEach(explanation.containerVolumes) { volume in
                        HStack {
                            Text(volume.path)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer()
                            Text(ScanFormat.size(volume.usedBytes))
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        .accessibilityElement(children: .combine)
                    }
                    Text("A scan of one volume can't count the others, such as the system volume, Preboot or VM, which holds swap files.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 520)
    }

    private var snapshotSummary: String {
        guard let snapshots = explanation.snapshots else { return "Unknown" }
        return snapshots.isEmpty ? "None" : "\(snapshots.count) local"
    }

    private var snapshotNote: String? {
        guard let snapshots = explanation.snapshots else { return "tmutil couldn't list them." }
        guard !snapshots.isEmpty else { return nil }
        // Oldest first. macOS reports no size for each one.
        let taken = snapshots.map { DiskExplanation.snapshotDate($0) ?? $0 }.sorted()
        return (taken + [
            "Each keeps the space of files deleted since it was taken. macOS removes each after about 24 hours, or sooner when space runs low.",
        ]).joined(separator: "\n")
    }

    private func row(_ title: String, _ value: String, _ note: String? = nil) -> some View {
        GridRow {
            Text(title)
                .foregroundStyle(.secondary)
                .gridColumnAlignment(.trailing)
            VStack(alignment: .leading, spacing: 2) {
                Text(value).monospacedDigit()
                if let note {
                    Text(note)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}
