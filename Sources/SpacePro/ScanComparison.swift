import Foundation
import SpaceCore

/// How an item of the current results differs from a saved scan.
public enum EntryChange: Sendable, Equatable {
    /// The saved scan has nothing of this name and kind here.
    case added
    /// The size changed by this many bytes, which can be zero or negative.
    case changed(Int64)
}

/// The changes in one folder of the current results.
public struct FolderChanges: Sendable {
    /// The folder's own change, or nil when the saved scan doesn't have the folder.
    public let total: Int64?
    /// Each child's change, by name.
    public let children: [String: EntryChange]
    /// Items the saved scan had in this folder that are gone, largest first.
    public let removed: [ScanEntry]

    public func change(for entry: ScanEntry) -> EntryChange? {
        children[entry.name]
    }

    /// The change an item adds to its folder, counting a new item's whole size.
    public func delta(for entry: ScanEntry, metric: SizeMetric) -> Int64 {
        switch change(for: entry) {
        case .added: Int64(clamping: entry.bytes(for: metric))
        case let .changed(delta): delta
        case nil: 0
        }
    }
}

/// A saved scan to compare the current results with. Items match by their names below
/// each root, as in `diskuse16x diff`, so the roots may be different folders.
public struct ScanComparison: Sendable {
    /// The saved scan's file name.
    public let name: String
    public let baseline: ScanReport
    /// When the saved scan was made, if known.
    public let date: Date?

    public init(name: String, baseline: ScanReport, date: Date? = nil) {
        self.name = name
        self.baseline = baseline
        self.date = date
    }

    /// The changes in `folder`, which `names` locate below the current root.
    public func changes(in folder: ScanEntry, at names: [String], metric: SizeMetric) -> FolderChanges {
        var saved: ScanEntry? = baseline.root
        for name in names {
            saved = saved?.children.first { $0.isDirectory && $0.name == name }
        }
        var before: [String: ScanEntry] = [:]
        for child in saved?.children ?? [] where before[child.name] == nil {
            before[child.name] = child
        }
        var children: [String: EntryChange] = [:]
        for child in folder.children {
            if let old = before[child.name], old.isDirectory == child.isDirectory {
                children[child.name] = .changed(difference(child.bytes(for: metric), old.bytes(for: metric)))
                before[child.name] = nil
            } else {
                children[child.name] = .added
            }
        }
        let removed = before.values.sorted {
            let (left, right) = ($0.bytes(for: metric), $1.bytes(for: metric))
            return left == right ? $0.name < $1.name : left > right
        }
        return FolderChanges(
            total: saved.map { difference(folder.bytes(for: metric), $0.bytes(for: metric)) },
            children: children,
            removed: removed
        )
    }
}

private func difference(_ new: UInt64, _ old: UInt64) -> Int64 {
    new >= old ? Int64(clamping: new - old) : -Int64(clamping: old - new)
}
