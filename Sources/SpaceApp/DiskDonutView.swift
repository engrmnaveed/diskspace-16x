import Charts
import SpaceCore
import SwiftUI

struct DiskDonutView: View {
    let workspace: ScanWorkspace
    let folder: ScanEntry

    var body: some View {
        let slices = makeSlices()
        Group {
            if workspace.isLoadingRows && slices.isEmpty {
                ProgressView("Preparing chart…")
                    .controlSize(.small)
            } else if slices.isEmpty {
                emptyState
            } else {
                GeometryReader { proxy in
                    let width = min(proxy.size.width, 860)
                    let compact = width < 535
                    ScrollView {
                        HStack(alignment: compact ? .top : .center, spacing: compact ? 14 : 24) {
                            ring(slices, width: min(width * 0.39, 274))
                                .padding(.top, compact ? 14 : 0)
                            legend(slices, compact: compact)
                        }
                        .padding(.horizontal, compact ? 16 : 24)
                        .padding(.vertical, 16)
                        .frame(width: width)
                        .frame(maxWidth: .infinity)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Folder allocation chart")
    }

    private func ring(_ slices: [Slice], width: CGFloat) -> some View {
        let diameter = min(width, 236)

        return ZStack {
            Chart(slices) { slice in
                SectorMark(
                    angle: .value("Allocated bytes", Double(slice.bytes)),
                    innerRadius: .ratio(0.68),
                    angularInset: slices.count > 1 ? 0.8 : 0
                )
                .foregroundStyle(slice.color)
            }
            .chartLegend(.hidden)
            .accessibilityHidden(true)

            VStack(spacing: 3) {
                Text(ScanFormat.size(folder.allocatedBytes))
                    .font(.system(size: 28, weight: .semibold).monospacedDigit())
                    .lineLimit(1)
                    .minimumScaleFactor(0.55)
                Text(folder.isPending ? "Allocated so far" : "Allocated in folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
            }
            .frame(maxWidth: diameter * 0.56)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Folder allocation")
            .accessibilityValue(ScanFormat.exactSize(folder.allocatedBytes))
            .help(ScanFormat.sizeDetail(folder.allocatedBytes))
        }
        .padding(5)
        .frame(width: diameter, height: diameter)
        .frame(width: width)
    }

    private func legend(_ slices: [Slice], compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Item")
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("Allocated")
                    .frame(width: 62, alignment: .trailing)
                if !compact {
                    Text("Share")
                        .frame(width: 47, alignment: .trailing)
                }
            }
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.leading, 10)
            .padding(.trailing, 29)
            .padding(.bottom, 6)
            .accessibilityHidden(true)

            ForEach(slices) { slice in
                legendRow(slice, compact: compact)
            }
        }
    }

    private func legendRow(_ slice: Slice, compact: Bool) -> some View {
        let isSelected = slice.entry.map { $0.id == workspace.selection } ?? false
        let share = Double(slice.bytes) / Double(folder.allocatedBytes)
        let percentFormat = FloatingPointFormatStyle<Double>.Percent.percent.precision(.fractionLength(0...1))
        let percent = share < 0.001 ? "<\(0.001.formatted(percentFormat))" : share.formatted(percentFormat)
        let valueLayout = compact
            ? AnyLayout(VStackLayout(alignment: .trailing, spacing: 1))
            : AnyLayout(HStackLayout(spacing: 7))

        return Button {
            if let entry = slice.entry {
                if entry.isDirectory {
                    workspace.browse(entry)
                } else {
                    workspace.selection = entry.id
                }
            } else {
                workspace.folderPresentation = .list
            }
        } label: {
            HStack(spacing: 7) {
                Circle()
                    .fill(slice.color)
                    .frame(width: 9, height: 9)
                Text(slice.title)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                valueLayout {
                    Text(ScanFormat.size(slice.bytes))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                        .frame(width: 62, alignment: .trailing)
                    Text(percent)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: 47, alignment: .trailing)
                }
                Image(systemName: slice.entry?.isDirectory == false ? "doc" : "chevron.right")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 12)
            }
            .font(.subheadline)
            .padding(.horizontal, 10)
            .padding(.vertical, compact ? 3 : 7)
            .background {
                RoundedRectangle(cornerRadius: 5)
                    .fill(isSelected ? SpaceColor.accent.opacity(0.16) : Color.clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(slice.title)
        .accessibilityValue("\(ScanFormat.exactSize(slice.bytes)), \(percent)")
        .accessibilityHint(slice.action)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .help("\(slice.title) · \(ScanFormat.sizeDetail(slice.bytes)) · \(percent)")
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "chart.pie")
                .font(.title)
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
            Text(emptyMessage)
                .font(.headline)
            Button("Show Folder List") { workspace.folderPresentation = .list }
                .buttonStyle(.link)
        }
        .padding(16)
    }

    private var emptyMessage: String {
        if folder.isCloudOnly { return "Cloud-only folder" }
        guard folder.isPending else { return "No allocated bytes in this folder" }
        return workspace.isScanning ? "No allocated bytes counted yet" : "No allocated bytes counted"
    }

    private struct Slice: Identifiable {
        let entry: ScanEntry?
        let bytes: UInt64
        let color: Color

        var id: String { entry?.id ?? "" }
        var title: String { entry?.name ?? "Other items" }
        var action: String {
            guard let entry else { return "Show the full folder list" }
            return entry.isDirectory ? "Open folder" : "Select file"
        }
    }

    private func makeSlices() -> [Slice] {
        let colors = [
            SpaceColor.accent,
            Color(nsColor: .systemBlue),
            Color(nsColor: .systemOrange),
            Color(nsColor: .systemPurple),
            Color(nsColor: .systemPink),
            Color(nsColor: .systemIndigo),
        ]
        guard !workspace.rows.isEmpty else { return [] }
        var remaining = folder.allocatedBytes
        var slices: [Slice] = []
        for (entry, color) in zip(workspace.rows, colors) {
            let bytes = min(entry.allocatedBytes, remaining)
            guard bytes > 0 else { break }
            slices.append(Slice(entry: entry, bytes: bytes, color: color))
            remaining -= bytes
        }
        if remaining > 0 {
            slices.append(Slice(entry: nil, bytes: remaining, color: Color(nsColor: .systemGray)))
        }
        return slices
    }
}
