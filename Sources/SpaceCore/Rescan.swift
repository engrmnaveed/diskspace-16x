import Foundation

extension ScanReport {
    /// This report with the folder `subtree` scanned again, as from
    /// `DiskScanner.rescan(folder:in:)`. Totals and item counts of every folder above it
    /// are recomputed, and issues at or below it are replaced. The excluded count keeps the
    /// earlier scan's figure. Returns nil when the folder isn't in this report.
    public func replacing(_ subtree: ScanReport) -> ScanReport? {
        let folder = subtree.root.path
        if folder == root.path {
            return ScanReport(
                root: subtree.root, issues: subtree.issues,
                excludedCount: max(excludedCount, subtree.excludedCount)
            )
        }
        guard folder.hasPrefix(root.path == "/" ? "/" : root.path + "/"),
              let updated = root.replacing(with: subtree.root)
        else { return nil }
        let prefix = folder + "/"
        let kept = issues.filter { $0.path != folder && !$0.path.hasPrefix(prefix) }
        return ScanReport(
            root: updated, issues: kept + subtree.issues,
            isPreview: isPreview, excludedCount: excludedCount
        )
    }
}

extension ScanEntry {
    /// This folder with the descendant folder at `replacement.path` swapped in.
    fileprivate func replacing(with replacement: ScanEntry) -> ScanEntry? {
        guard let index = children.firstIndex(where: {
            replacement.path == $0.path || replacement.path.hasPrefix($0.path + "/")
        }) else { return nil }
        let old = children[index]
        let new: ScanEntry
        if old.path == replacement.path {
            guard old.isDirectory else { return nil }
            new = replacement
        } else {
            guard let nested = old.replacing(with: replacement) else { return nil }
            new = nested
        }
        var updated = children
        updated[index] = new
        return ScanEntry(
            path: path, name: name, isDirectory: isDirectory, isHardLinkAlias: isHardLinkAlias,
            isCloudOnly: isCloudOnly, isPending: updated.contains(where: \.isPending),
            logicalBytes: logicalBytes.swapping(old.logicalBytes, for: new.logicalBytes),
            allocatedBytes: allocatedBytes.swapping(old.allocatedBytes, for: new.allocatedBytes),
            itemCount: itemCount.swapping(old.countedItems, for: new.countedItems),
            children: updated
        )
    }
}

private extension UInt64 {
    /// This total with `old` taken out and `new` added, clamped to the valid range.
    func swapping(_ old: UInt64, for new: UInt64) -> UInt64 {
        let without = self > old ? self - old : 0
        let (sum, overflow) = without.addingReportingOverflow(new)
        return overflow ? .max : sum
    }
}
