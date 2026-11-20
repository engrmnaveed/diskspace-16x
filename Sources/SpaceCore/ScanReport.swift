import Foundation

public enum SizeMetric: String, CaseIterable, Sendable, Codable {
    case allocated
    case logical
}

public struct ScanEntry: Identifiable, Sendable {
    public var id: String { path }
    public let path: String
    public let name: String
    public let isDirectory: Bool
    /// A later directory entry for an inode whose bytes already contribute elsewhere in the scan.
    public let isHardLinkAlias: Bool
    public let isCloudOnly: Bool
    /// The preview still has local entries to inspect for this item.
    public let isPending: Bool
    public let logicalBytes: UInt64
    public let allocatedBytes: UInt64
    /// Files and folders below this entry, counting each hard-linked file once.
    public let itemCount: UInt64
    public let children: [ScanEntry]

    public func bytes(for metric: SizeMetric) -> UInt64 {
        switch metric {
        case .allocated: allocatedBytes
        case .logical: logicalBytes
        }
    }

    public func rankedChildren(by metric: SizeMetric) -> [ScanEntry] {
        children.sorted { $0.ranksBefore($1, by: metric) }
    }

    fileprivate func ranksBefore(_ other: ScanEntry, by metric: SizeMetric) -> Bool {
        let left = bytes(for: metric)
        let right = other.bytes(for: metric)
        if left == right {
            return path.utf8.lexicographicallyPrecedes(other.path.utf8)
        }
        return left > right
    }
}

/// How a list of entries is ordered for display.
public enum EntryOrder: String, CaseIterable, Sendable {
    case size
    case items
    case name
}

extension Array where Element == ScanEntry {
    /// Orders entries by size, item count, or name. Ties keep a stable order by path bytes,
    /// as the CLI does. Names compare like Finder, ignoring case and treating digits as numbers.
    public func ordered(by order: EntryOrder, ascending: Bool, metric: SizeMetric) -> [ScanEntry] {
        sorted { left, right in
            let comparison: ComparisonResult
            switch order {
            case .size:
                comparison = threeWay(left.bytes(for: metric), right.bytes(for: metric))
            case .items:
                comparison = threeWay(left.itemCount, right.itemCount)
            case .name:
                comparison = left.name.localizedStandardCompare(right.name)
            }
            if comparison == .orderedSame {
                return left.path.utf8.lexicographicallyPrecedes(right.path.utf8)
            }
            return (comparison == .orderedAscending) == ascending
        }
    }
}

private func threeWay(_ left: UInt64, _ right: UInt64) -> ComparisonResult {
    left == right ? .orderedSame : (left < right ? .orderedAscending : .orderedDescending)
}

public struct ScanIssue: Identifiable, Sendable, Encodable {
    /// Why a path was skipped. Access refusals are expected when scanning system locations.
    public enum Kind: String, Sendable, Encodable {
        /// File permissions don't let this user read the path (EACCES).
        case permissionDenied
        /// macOS privacy controls or System Integrity Protection refused access (EPERM).
        case notPermitted
        /// Any other failure, such as an I/O error or an unresponsive cloud provider.
        case error
    }

    public var id: String { path + "\0" + message }
    public let path: String
    public let message: String
    public let kind: Kind

    /// The OS refused access, so the path is protected rather than failed.
    public var isProtected: Bool { kind != .error }

    init(path: String, message: String, kind: Kind = .error) {
        self.path = path
        self.message = message
        self.kind = kind
    }
}

public struct ScanProgress: Sendable {
    public let visitedEntries: UInt64
    public let currentPath: String
}

public struct ScanReport: Sendable {
    public let root: ScanEntry
    public let issues: [ScanIssue]
    public let isPreview: Bool
    /// Entries skipped by exclude patterns. Their contents aren't visited or counted.
    public let excludedCount: UInt64
    /// Any skipped path leaves the totals incomplete, including protected ones.
    public var isPartial: Bool { !issues.isEmpty }
    /// Failures other than access refusals. These warrant a warning.
    public var errorIssues: [ScanIssue] { issues.filter { !$0.isProtected } }
    /// Paths the OS refused to let this scan read.
    public var protectedIssues: [ScanIssue] { issues.filter(\.isProtected) }

    init(root: ScanEntry, issues: [ScanIssue], isPreview: Bool = false, excludedCount: UInt64 = 0) {
        self.root = root
        self.issues = issues
        self.isPreview = isPreview
        self.excludedCount = excludedCount
    }

    /// Ranks each counted regular file in the selected subtree. Hard-link ownership
    /// is determined by the full scan and is retained when browsing a subtree.
    public func largestFiles(limit: Int, by metric: SizeMetric, under entry: ScanEntry? = nil) -> [ScanEntry] {
        guard limit > 0 else { return [] }
        let scope = entry ?? root
        if !scope.isDirectory {
            return scope.isHardLinkAlias || scope.isCloudOnly || scope.isPending ? [] : [scope]
        }

        var top = TopFiles(limit: limit, metric: metric)
        var levels = [scope.children[...]]
        while let level = levels.indices.last {
            if Task.isCancelled { return [] }
            guard let child = levels[level].popFirst() else {
                levels.removeLast()
                continue
            }
            if child.isDirectory {
                levels.append(child.children[...])
            } else if !child.isHardLinkAlias, !child.isCloudOnly, !child.isPending {
                top.consider(child)
            }
        }
        return top.ranked
    }
}

/// The least relevant result is at the root so a small limit stays inexpensive on large scans.
private struct TopFiles {
    let limit: Int
    let metric: SizeMetric
    private var heap: [ScanEntry] = []

    init(limit: Int, metric: SizeMetric) {
        self.limit = limit
        self.metric = metric
    }

    var ranked: [ScanEntry] {
        heap.sorted { $0.ranksBefore($1, by: metric) }
    }

    mutating func consider(_ entry: ScanEntry) {
        if heap.count < limit {
            heap.append(entry)
            siftUp(from: heap.count - 1)
        } else if entry.ranksBefore(heap[0], by: metric) {
            heap[0] = entry
            siftDown()
        }
    }

    private mutating func siftUp(from index: Int) {
        var child = index
        while child > 0 {
            let parent = (child - 1) / 2
            guard heap[parent].ranksBefore(heap[child], by: metric) else { break }
            heap.swapAt(parent, child)
            child = parent
        }
    }

    private mutating func siftDown() {
        var parent = 0
        while parent < heap.count / 2 {
            let left = parent * 2 + 1
            let right = left + 1
            let worseChild = right < heap.count && heap[left].ranksBefore(heap[right], by: metric)
                ? right
                : left
            guard heap[parent].ranksBefore(heap[worseChild], by: metric) else { break }
            heap.swapAt(parent, worseChild)
            parent = worseChild
        }
    }
}
