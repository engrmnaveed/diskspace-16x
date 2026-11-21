import Foundation
import SpaceCore

/// A Build Footprint receipt: what changed on disk between a saved scan and the current
/// results, labelled with the operation measured, such as "first Xcode build", as
/// Markdown to keep or share. Paths are relative to the scanned folders, so the receipt
/// doesn't show where they are.
public enum FootprintReceipt {
    /// A file whose size changed, or that only one scan has.
    public struct FileChange: Equatable, Sendable {
        /// The path below the scanned folder.
        public let path: String
        public let before: UInt64?
        public let after: UInt64?

        public var delta: Int64 {
            let (new, old) = (after ?? 0, before ?? 0)
            return new >= old ? Int64(clamping: new - old) : -Int64(clamping: old - new)
        }
    }

    /// Every file that grew, shrank, appeared or disappeared, matched by its path below
    /// each root, largest change first with ties in path order.
    public static func fileChanges(before: ScanEntry, after: ScanEntry, metric: SizeMetric) -> [FileChange] {
        var old: [String: UInt64] = [:]
        forEachFile(below: before) { path, bytes in old[path] = bytes }
        var changes: [FileChange] = []
        forEachFile(below: after) { path, bytes in
            let previous = old.removeValue(forKey: path)
            if previous != bytes { changes.append(FileChange(path: path, before: previous, after: bytes)) }
        }
        for (path, bytes) in old where bytes > 0 {
            changes.append(FileChange(path: path, before: bytes, after: nil))
        }
        return changes.sorted {
            $0.delta.magnitude == $1.delta.magnitude
                ? $0.path.utf8.lexicographicallyPrecedes($1.path.utf8)
                : $0.delta.magnitude > $1.delta.magnitude
        }

        func forEachFile(below root: ScanEntry, _ body: (String, UInt64) -> Void) {
            var pending: [(ScanEntry, String)] = root.children.map { ($0, $0.name) }
            while let (entry, path) = pending.popLast() {
                if entry.isDirectory {
                    pending.append(contentsOf: entry.children.map { ($0, path + "/" + $0.name) })
                } else if !entry.isHardLinkAlias {
                    body(path, entry.bytes(for: metric))
                }
            }
        }
    }

    /// The receipt in Markdown, with the `limit` largest file changes.
    public static func markdown(
        label: String, comparison: ScanComparison, after: ScanReport, afterDate: Date?,
        metric: SizeMetric, version: String, limit: Int = 25
    ) -> String {
        let before = comparison.baseline
        let changes = fileChanges(before: before.root, after: after.root, metric: metric)
        let gained = changes.reduce(UInt64(0)) { $0 + UInt64(max(0, $1.delta)) }
        let freed = changes.reduce(UInt64(0)) { $0 + UInt64(max(0, -$1.delta)) }
        let (oldBytes, newBytes) = (before.root.bytes(for: metric), after.root.bytes(for: metric))
        let total = FileChange(path: "", before: oldBytes, after: newBytes).delta
        let items = Int64(clamping: after.root.itemCount) - Int64(clamping: before.root.itemCount)
        let title = label.trimmingCharacters(in: .whitespacesAndNewlines)
        let measure = metric == .allocated ? "Allocated" : "Apparent"

        var lines = [
            "# Build Footprint: \(cell(title.isEmpty ? "Untitled operation" : title))",
            "",
            "| | Scan | \(measure) size | Items |",
            "| --- | --- | ---: | ---: |",
            "| Before | \(cell(stamp(comparison.date))) (“\(cell(comparison.name))”) | \(size(oldBytes)) | \(count(before.root.itemCount)) |",
            "| After | \(cell(stamp(afterDate))) | \(size(newBytes)) | \(count(after.root.itemCount)) |",
            "| Change | | \(change(total)) | \(items >= 0 ? "+" : "−")\(count(items.magnitude)) |",
            "",
            "Files gained \(size(gained)) and lost \(size(freed)): \(changes.count.formatted()) \(changes.count == 1 ? "file" : "files") changed.",
        ]
        if before.root.path != after.root.path {
            lines.append("")
            lines.append("The scans are of different folders, compared by the paths below each one.")
        }
        for (name, report) in [("before", before), ("after", after)] where report.isPartial {
            lines.append("")
            lines.append("The \(name) scan is partial: \(report.issues.count.formatted()) paths couldn't be read, so changes there are uncertain.")
        }
        if !changes.isEmpty {
            lines += [
                "",
                "## Largest changes",
                "",
                "| Change | Before | After | Path |",
                "| ---: | ---: | ---: | --- |",
            ]
            for item in changes.prefix(limit) {
                lines.append(
                    "| \(change(item.delta)) | \(item.before.map(size) ?? "–") | \(item.after.map(size) ?? "–") | \(cell(item.path)) |"
                )
            }
            if changes.count > limit {
                lines.append("")
                lines.append("And \((changes.count - limit).formatted()) smaller changes.")
            }
        }
        lines += [
            "",
            "Paths are relative to the scanned folder. Other programs can change the same folders between the scans, so this receipt shows what changed between the scan times, not what one program did.",
            "",
            "Made with Space 16X \(version).",
            "",
        ]
        return lines.joined(separator: "\n")
    }

    private static func size(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }

    private static func change(_ delta: Int64) -> String {
        delta == 0 ? "0 bytes" : (delta > 0 ? "+" : "−") + size(delta.magnitude)
    }

    private static func count(_ value: UInt64) -> String { value.formatted() }

    private static func stamp(_ date: Date?) -> String {
        guard let date else { return "Unknown time" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }

    /// Text for a Markdown table cell, where a pipe or line break would end the cell.
    private static func cell(_ text: String) -> String {
        text.replacingOccurrences(of: "|", with: "\\|").replacingOccurrences(of: "\n", with: " ")
    }
}
