import SpaceCore
import SwiftUI

struct DiskTreemapView: View {
    let workspace: ScanWorkspace
    let folder: ScanEntry

    var body: some View {
        let tiles = makeTiles()
        VStack(alignment: .leading, spacing: 6) {
            GeometryReader { proxy in
                ZStack {
                    RoundedRectangle(cornerRadius: 9)
                        .fill(.quaternary.opacity(0.35))
                        .accessibilityHidden(true)
                    if workspace.isLoadingRows && workspace.rows.isEmpty {
                        ProgressView("Ranking items…")
                            .controlSize(.small)
                    } else if tiles.isEmpty {
                        Text(folder.isPending && workspace.isScanning
                            ? "Waiting for \(sizeWord) sizes…" : "No \(sizeWord) bytes counted here")
                            .font(.headline)
                            .foregroundStyle(.secondary)
                    } else {
                        let bounds = CGRect(origin: .zero, size: proxy.size).insetBy(dx: 2, dy: 2)
                        let rectangles = TreemapLayout.rectangles(in: bounds, weights: tiles.map(\.weight))
                        ForEach(tiles.indices, id: \.self) { index in
                            tile(tiles[index], rectangle: rectangles[index])
                                .accessibilitySortPriority(Double(tiles.count - index))
                        }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            HStack {
                Text(folder.isPending
                    ? "Area shows \(sizeWord) size counted so far."
                    : "Area represents \(sizeWord) size.")
                    .foregroundStyle(.secondary)
                Spacer(minLength: 5)
                Button("Show All Items") { workspace.folderPresentation = .list }
                    .buttonStyle(.borderless)
                    .help("Switch to the full folder list (⌘1)")
            }
            .font(.caption)
        }
        .padding(.horizontal, 24)
        .padding(.top, 4)
        .padding(.bottom, 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Folder treemap")
    }

    private enum Tile {
        case item(entry: ScanEntry, bytes: UInt64)
        case other(bytes: UInt64)

        var bytes: UInt64 {
            switch self {
            case let .item(_, bytes), let .other(bytes): bytes
            }
        }

        var weight: CGFloat { CGFloat(bytes) }
    }

    private var sizeWord: String { workspace.sizeMetric.title.lowercased() }

    private func makeTiles() -> [Tile] {
        let metric = workspace.sizeMetric
        let total = folder.bytes(for: metric)
        guard total > 0 else { return [] }
        var tiles: [Tile] = []
        var remaining = total
        let minimumBytes = max(total / 250, 1)
        for entry in workspace.chartRows {
            let bytes = min(entry.bytes(for: metric), remaining)
            guard bytes >= minimumBytes, remaining > 0 else { break }
            tiles.append(.item(entry: entry, bytes: bytes))
            remaining -= bytes
        }
        if remaining > 0 { tiles.append(.other(bytes: remaining)) }
        return tiles
    }

    @ViewBuilder
    private func tile(_ tile: Tile, rectangle: CGRect) -> some View {
        if rectangle.width >= 4 && rectangle.height >= 4 {
            Group {
                switch tile {
                case let .item(entry, bytes):
                    Button {
                        if entry.isDirectory {
                            workspace.browse(entry)
                        } else {
                            workspace.selection = entry.id
                        }
                    } label: {
                        tileLabel(
                            entry.name, bytes: bytes, rectangle: rectangle,
                            symbol: entry.isDirectory ? "folder.fill" : "doc",
                            color: entry.isDirectory ? SpaceColor.accent : Color(nsColor: .systemBlue),
                            isSelected: entry.id == workspace.selection
                        )
                    }
                    .accessibilityLabel(entry.name)
                    .accessibilityValue(ScanFormat.exactSize(bytes))
                    .accessibilityHint(entry.isDirectory ? "Open folder" : "Select file")
                    .accessibilityAddTraits(entry.id == workspace.selection ? .isSelected : [])
                    .help("\(entry.path) • \(ScanFormat.size(bytes)) · \(ScanFormat.sizeDetail(bytes))")
                    .contextMenu {
                        if entry.isDirectory {
                            Button("Open Folder") { workspace.browse(entry) }
                        }
                        Button("Reveal in Finder") { workspace.reveal(entry) }
                        Button("Copy Path") { workspace.copyPath(of: entry) }
                    }
                case let .other(bytes):
                    Button {
                        workspace.folderPresentation = .list
                    } label: {
                        tileLabel(
                            "Other Items", bytes: bytes, rectangle: rectangle,
                            symbol: "ellipsis", color: .secondary
                        )
                    }
                    .accessibilityLabel("Other items")
                    .accessibilityValue(ScanFormat.exactSize(bytes))
                    .accessibilityHint("Show all items in the folder list")
                    .help("View the full folder list")
                }
            }
            .buttonStyle(.plain)
            .position(x: rectangle.midX, y: rectangle.midY)
        }
    }

    private func tileLabel(
        _ title: String, bytes: UInt64, rectangle: CGRect, symbol: String,
        color: Color, isSelected: Bool = false
    ) -> some View {
        let hasLabelRoom = rectangle.width >= 54 && rectangle.height >= 22
        let hasSizeRoom = rectangle.width >= 66 && rectangle.height >= 48
        return VStack(alignment: .leading, spacing: 3) {
            if hasLabelRoom {
                Label(title, systemImage: symbol)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            if hasSizeRoom {
                Text(ScanFormat.size(bytes))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, hasLabelRoom ? 9 : 0)
        .padding(.vertical, hasSizeRoom ? 7 : 0)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .frame(width: rectangle.width - 3, height: rectangle.height - 3)
        .background {
            RoundedRectangle(cornerRadius: 5)
                .fill(color.opacity(isSelected ? 0.33 : 0.16))
        }
        .overlay {
            RoundedRectangle(cornerRadius: 5)
                .strokeBorder(color.opacity(isSelected ? 0.95 : 0.32), lineWidth: isSelected ? 2 : 1)
        }
        .contentShape(RoundedRectangle(cornerRadius: 5))
    }
}

enum TreemapLayout {
    /// Recursively splits the longer edge near the halfway weight, keeping the ranked order.
    static func rectangles(in bounds: CGRect, weights: [CGFloat]) -> [CGRect] {
        var output: [CGRect] = []
        output.reserveCapacity(weights.count)
        split(bounds, atWeights: weights[...], into: &output)
        return output
    }

    private static func split(_ bounds: CGRect, atWeights weights: ArraySlice<CGFloat>, into output: inout [CGRect]) {
        guard let firstWeight = weights.first else { return }
        guard weights.count > 1 else {
            output.append(bounds)
            return
        }

        let total = weights.reduce(0, +)
        var leftWeight = firstWeight
        var pivot = weights.startIndex + 1
        while pivot < weights.endIndex - 1, leftWeight + weights[pivot] / 2 < total / 2 {
            leftWeight += weights[pivot]
            pivot += 1
        }
        let edge: CGRectEdge = bounds.width >= bounds.height ? .minXEdge : .minYEdge
        let length = edge == .minXEdge ? bounds.width : bounds.height
        let regions = bounds.divided(atDistance: length * (leftWeight / total), from: edge)
        split(regions.slice, atWeights: weights[..<pivot], into: &output)
        split(regions.remainder, atWeights: weights[pivot...], into: &output)
    }
}
