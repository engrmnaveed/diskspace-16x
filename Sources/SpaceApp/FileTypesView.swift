import SpaceCore
import SpacePro
import SwiftUI

/// Pro: the open folder's file types, largest first, with their size, file count and share.
struct FileTypesView: View {
    @Bindable var workspace: ScanWorkspace
    let folder: ScanEntry
    @ViewState private var types: [FileTypeTotal] = []
    @ViewState private var isLoading = true

    /// Changes whenever the folder's contents or the size metric do.
    private var key: String {
        "\(folder.path)\u{0}\(folder.allocatedBytes)\u{0}\(folder.itemCount)\u{0}\(workspace.sizeMetric.rawValue)"
    }

    var body: some View {
        let metric = workspace.sizeMetric
        let total = folder.bytes(for: metric)
        Table(types) {
            TableColumn("Type") { type in
                Text(type.label)
                    .foregroundStyle(type.name.isEmpty ? .secondary : .primary)
            }
            .width(min: 140, ideal: 220)

            TableColumn(metric.title) { type in
                Text(ScanFormat.size(type.bytes(for: metric)))
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .help(ScanFormat.exactSize(type.bytes(for: metric)))
            }
            .width(min: 88, ideal: 108, max: 164)

            TableColumn("Files") { type in
                Text(type.fileCount.formatted())
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .width(min: 60, ideal: 80, max: 120)

            TableColumn("Share") { type in
                let ratio = total == 0 ? 0 : min(Double(type.bytes(for: metric)) / Double(total), 1)
                Text(ratio.formatted(.percent.precision(.fractionLength(0...1))))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .width(min: 60, ideal: 80, max: 120)
        }
        .tableStyle(.inset(alternatesRowBackgrounds: false))
        .accessibilityLabel("File types")
        .overlay {
            if isLoading && types.isEmpty {
                ProgressView("Counting file types…")
                    .controlSize(.small)
                    .padding(16)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            } else if types.isEmpty {
                Text("No files in this folder.")
                    .foregroundStyle(.secondary)
            }
        }
        .task(id: key) {
            isLoading = true
            let folder = folder
            let worker = Task.detached(priority: .userInitiated) {
                FileTypes.breakdown(of: folder, metric: metric)
            }
            let result = await withTaskCancellationHandler {
                await worker.value
            } onCancel: {
                worker.cancel()
            }
            guard !Task.isCancelled else { return }
            types = result
            isLoading = false
        }
    }
}
