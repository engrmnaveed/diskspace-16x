import Foundation

/// Why Space 16X keeps an item out of the Trash. The rules match the CLI explorer's.
public enum TrashBlocker: Sendable, Equatable {
    case cloudOnly
    case pending
    case protected
    case incomplete

    /// Completes a sentence that starts with the item's name.
    public var reason: String {
        switch self {
        case .cloudOnly: "is cloud-only. Remove it in Finder or its cloud app."
        case .pending: "wasn't scanned, so its size is unknown."
        case .protected: "is protected, so Space 16X couldn't see all of it."
        case .incomplete: "had read errors, so its size is uncertain."
        }
    }
}

extension ScanReport {
    /// Why `entry` can't be moved to the Trash, or nil if it can. Cloud-only and unscanned
    /// items are kept, and so is anything at or above a path the scan skipped.
    public func trashBlocker(for entry: ScanEntry) -> TrashBlocker? {
        if entry.isCloudOnly { return .cloudOnly }
        if entry.isPending { return .pending }
        var blocker: TrashBlocker?
        for issue in issues where issue.path.isAtOrBelow(entry.path) {
            guard issue.isProtected else { return .incomplete }
            blocker = .protected
        }
        return blocker
    }

    /// A copy without the entries at `paths`, as after moving them to the Trash. Every
    /// folder above them loses their bytes and items, and their skipped paths are dropped.
    public func removing(_ paths: Set<String>) -> ScanReport {
        guard !paths.isEmpty else { return self }
        var folders = Set<String>()
        for path in paths {
            var folder = Substring(path)
            while folder.count > root.path.count, let slash = folder.lastIndex(of: "/") {
                folder = folder[..<slash]
                folders.insert(String(folder))
            }
        }
        return ScanReport(
            root: root.removing(paths, below: folders),
            issues: issues.filter { issue in !paths.contains { issue.path.isAtOrBelow($0) } },
            isPreview: isPreview,
            excludedCount: excludedCount
        )
    }
}

extension ScanEntry {
    /// Files and folders this entry adds to its parent's item count.
    fileprivate var countedItems: UInt64 {
        guard !isHardLinkAlias else { return 0 }
        return itemCount == .max ? .max : itemCount + 1
    }

    /// Removes the children at `paths`, descending only into `folders` that hold some.
    fileprivate func removing(_ paths: Set<String>, below folders: Set<String>) -> ScanEntry {
        var logical = logicalBytes
        var allocated = allocatedBytes
        var items = itemCount
        var kept: [ScanEntry] = []
        kept.reserveCapacity(children.count)
        for child in children {
            if paths.contains(child.path) {
                logical = logical.reduced(by: child.logicalBytes)
                allocated = allocated.reduced(by: child.allocatedBytes)
                items = items.reduced(by: child.countedItems)
            } else if child.isDirectory, folders.contains(child.path) {
                let updated = child.removing(paths, below: folders)
                logical = logical.reduced(by: child.logicalBytes.reduced(by: updated.logicalBytes))
                allocated = allocated.reduced(by: child.allocatedBytes.reduced(by: updated.allocatedBytes))
                items = items.reduced(by: child.itemCount.reduced(by: updated.itemCount))
                kept.append(updated)
            } else {
                kept.append(child)
            }
        }
        return ScanEntry(
            path: path, name: name, isDirectory: isDirectory, isHardLinkAlias: isHardLinkAlias,
            isCloudOnly: isCloudOnly, isPending: isPending,
            logicalBytes: logical, allocatedBytes: allocated, itemCount: items, children: kept
        )
    }
}

private extension UInt64 {
    func reduced(by amount: UInt64) -> UInt64 { self > amount ? self - amount : 0 }
}

private extension String {
    func isAtOrBelow(_ folder: String) -> Bool {
        self == folder || hasPrefix(folder.hasSuffix("/") ? folder : folder + "/")
    }
}
