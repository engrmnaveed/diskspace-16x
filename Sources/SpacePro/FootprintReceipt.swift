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

    /// The UserDefaults key for each operation's last budget, by its lowercased name.
    public static let budgetsKey = "footprintBudgets"

    /// Parses a budget such as `500 MB`, `2GB`, `1.5 G` or `750000`, in decimal units as
    /// `space16x diff --budget` takes them. Nil for an empty or unreadable size.
    public static func parseBudget(_ text: String) -> UInt64? {
        let compact = text.uppercased().filter { !$0.isWhitespace }
        let number = compact.hasSuffix("B") ? String(compact.dropLast()) : compact
        guard let unit = number.last else { return nil }
        let scale: Double
        var digits = number
        switch unit {
        case "K": scale = 1e3
        case "M": scale = 1e6
        case "G": scale = 1e9
        case "T": scale = 1e12
        default: scale = 1
        }
        if scale != 1 { digits.removeLast() }
        guard !digits.isEmpty, digits.allSatisfy({ $0.isNumber || $0 == "." }), let value = Double(digits) else {
            return nil
        }
        let bytes = (value * scale).rounded()
        return bytes < 1.8e19 ? UInt64(bytes) : nil
    }

    /// Whether growing by `delta` bytes exceeds `budget`, as `space16x diff --budget` decides.
    public static func isOverBudget(_ delta: Int64, budget: UInt64) -> Bool {
        delta > 0 && UInt64(delta) > budget
    }

    /// The receipt in Markdown, with the `limit` largest file changes and, with a budget,
    /// whether the operation stayed within it.
    public static func markdown(
        label: String, comparison: ScanComparison, after: ScanReport, afterDate: Date?,
        metric: SizeMetric, version: String, limit: Int = 25, budget: UInt64? = nil
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
        if let budget {
            lines.append("")
            if isOverBudget(total, budget: budget) {
                lines.append("**Over budget:** the operation grew \(size(total.magnitude)), \(size(total.magnitude - budget)) more than its \(size(budget)) budget.")
            } else {
                let spare = budget - UInt64(max(0, total))
                lines.append("**Within budget:** the operation's \(size(budget)) budget has \(size(spare)) to spare.")
            }
        }
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
