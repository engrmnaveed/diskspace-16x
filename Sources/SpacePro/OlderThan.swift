import Foundation
import SpaceCore

/// Pro: the files last modified more than some days ago, as `space16x scan --older-than`
/// counts them, for **View → Show Files Older Than**.
public enum OlderThan {
    /// The presets the View menu offers, in days.
    public static let presets = [30, 90, 182, 365, 730]

    /// The time `days` days before `now`, in seconds since 1970.
    public static func cutoff(days: Int, now: Date = Date()) -> UInt32 {
        let seconds = now.timeIntervalSince1970 - Double(days) * 86_400
        return UInt32(clamping: Int64(max(0, seconds)))
    }

    /// The report with only the files last modified before `cutoff`, and the folders that
    /// hold them, as the CLI's `modified::older_than` keeps them. Folder totals, item counts
    /// and times count those files alone. Folders that weren't read completely stay, so
    /// their state still shows. Files without a time, as in opened scans, don't count.
    public static func filter(_ report: ScanReport, cutoff: UInt32) -> ScanReport {
        let issuePaths = Set(report.issues.map(\.path))
        var removed = Set<String>()
        /// Whether `folder` keeps anything. A folder that doesn't is removed whole by its
        /// parent, so only kept folders list their removed children.
        func visit(_ folder: ScanEntry) -> Bool {
            var keeps = false
            var dropped: [String] = []
            for child in folder.children {
                if child.isDirectory {
                    if visit(child) || child.isPending || child.isCloudOnly || issuePaths.contains(child.path) {
                        keeps = true
                    } else {
                        dropped.append(child.path)
                    }
                } else if child.newestModified != 0, child.newestModified < cutoff {
                    keeps = true
                } else {
                    dropped.append(child.path)
                }
            }
            if keeps || folder.path == report.root.path { removed.formUnion(dropped) }
            return keeps
        }
        _ = visit(report.root)
        return report.removing(removed)
    }
}
