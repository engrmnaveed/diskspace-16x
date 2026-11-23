import Foundation
import SpaceCore

/// Pro: one self-contained HTML page to share a scan, as `diskuse16x scan --html` writes it:
/// a summary, a treemap of the largest items, the scan root's contents, the largest files
/// and file types. The page has no scripts and loads nothing, and it follows the reader's
/// light or dark appearance. Owners, which the CLI adds on shared servers, stay out: the
/// Mac app scans one user's files. `fixtures/html-report` holds the structure both apps
/// test.
public enum HTMLReport {
    /// The most items the treemap draws. The rest share one cell.
    static let mapItems = 30
    /// The most rows each table lists.
    static let tableRows = 50
    static let topFiles = 25
    static let types = 12

    /// The same style sheet as the CLI's page.
    static let style = ":root{color-scheme:light dark;--bg:#fbfbfa;--fg:#1d2321;--muted:#66706c;--line:#dde3e0;--accent:#0f8f78;--folder:#2fae93;--file:#7a8b86;--other:#b9c4c0}@media (prefers-color-scheme:dark){:root{--bg:#161a19;--fg:#e8eeeb;--muted:#9aa6a1;--line:#2c3431;--accent:#4fd1b3;--folder:#1f8f78;--file:#56645f;--other:#3a4541}}*{box-sizing:border-box}body{margin:0;padding:32px 20px;background:var(--bg);color:var(--fg);font:15px/1.45 -apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,sans-serif}main{max-width:1040px;margin:0 auto}h1{font-size:26px;margin:0 0 4px;word-break:break-all}h2{font-size:18px;margin:32px 0 10px}p{margin:4px 0;color:var(--muted)}.note{color:#b7791f}.map{position:relative;height:420px;border-radius:10px;overflow:hidden;background:var(--line)}.cell{position:absolute;border:1px solid var(--bg);overflow:hidden;padding:4px 6px;color:#fff;font-size:12px;line-height:1.3}.cell b{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder{background:var(--folder)}.file{background:var(--file)}.other{background:var(--other);color:var(--fg)}table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums}th,td{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--muted);font-weight:600}td.n,th.n{text-align:right;white-space:nowrap}td.p{word-break:break-all}.bar{height:6px;border-radius:3px;background:var(--accent)}footer{margin-top:36px;color:var(--muted);font-size:13px}"

    /// The page for `report`, ranked by `metric`.
    public static func page(
        report: ScanReport, metric: SizeMetric, excludes: [String], olderThanDays: Int?,
        version: String, scannedAt: Date = Date()
    ) -> String {
        let root = report.root
        let measure = metric == .allocated ? "allocated" : "apparent"
        var html = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">"
        html += "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
        html += "<title>\(escape(root.name)) · DiskUse16x</title><style>\(style)</style></head><body><main>"
        html += "<h1>\(escape(root.path))</h1>"
        let total = size(root.bytes(for: metric))
        let apparent = size(root.logicalBytes)
        let items = thousands(root.itemCount)
        html += "<p>\(total) \(measure) · \(apparent) apparent · \(items) items · scanned \(stamp(scannedAt))</p>"
        if !report.issues.isEmpty {
            let count = report.issues.count
            html += "<p class=\"note\">Partial scan: \(thousands(UInt64(count))) \(count == 1 ? "path" : "paths") couldn't be read, so the totals leave them out.</p>"
        }
        if let days = olderThanDays {
            html += "<p>Counts only files last modified more than \(days) days ago.</p>"
        }
        if !excludes.isEmpty {
            html += "<p>Excluded: \(excludes.map(escape).joined(separator: ", "))</p>"
        }
        let children = root.children.sorted { left, right in
            let (a, b) = (left.bytes(for: metric), right.bytes(for: metric))
            return a == b ? left.name.utf8.lexicographicallyPrecedes(right.name.utf8) : a > b
        }
        html += map(children, metric: metric)
        html += contents(children, metric: metric, total: root.bytes(for: metric))
        html += files(report, metric: metric)
        html += fileTypes(root, metric: metric)
        html += "<footer>Made with DiskUse16x \(escape(version)) (Mac, Pro). Sizes use decimal units (1 KB = 1,000 bytes). "
        html += "Review the paths on this page before you share it.</footer></main></body></html>\n"
        return html
    }

    private static func map(_ children: [ScanEntry], metric: SizeMetric) -> String {
        var cells: [(name: String, bytes: UInt64, kind: String)] = children.prefix(mapItems)
            .filter { $0.bytes(for: metric) > 0 }
            .map { ($0.name, $0.bytes(for: metric), $0.isDirectory ? "folder" : "file") }
        let rest = children.dropFirst(mapItems)
        let restBytes = rest.reduce(UInt64(0)) { $0 + $1.bytes(for: metric) }
        if restBytes > 0 {
            cells.append(("\(rest.count) more items", restBytes, "other"))
            cells.sort { $0.bytes > $1.bytes }
        }
        guard !cells.isEmpty else { return "" }
        var html = "<h2>Treemap</h2><div class=\"map\">"
        let rects = layout(cells.map { Double($0.bytes) }, width: 100, height: 100)
        for (cell, rect) in zip(cells, rects) {
            let label = "\(escape(cell.name)) · \(size(cell.bytes))"
            let place = "left:\(percent(rect.x))%;top:\(percent(rect.y))%;width:\(percent(rect.width))%;height:\(percent(rect.height))%"
            html += "<div class=\"cell \(cell.kind)\" style=\"\(place)\" title=\"\(label)\">"
            html += "<b>\(escape(cell.name))</b>\(size(cell.bytes))</div>"
        }
        return html + "</div>"
    }

    private static func contents(_ children: [ScanEntry], metric: SizeMetric, total: UInt64) -> String {
        var html = "<h2>Contents</h2><table><tr><th>Name</th><th class=\"n\">Size</th>"
        html += "<th class=\"n\">Items</th><th class=\"n\">Share</th><th style=\"width:18%\"></th></tr>"
        for child in children.prefix(tableRows) {
            let bytes = child.bytes(for: metric)
            let share = total == 0 ? 0 : Double(bytes) * 100 / Double(total)
            let shareText = String(format: "%.1f", share)
            let name = escape(child.name) + (child.isDirectory ? "/" : "")
            let items = child.isDirectory ? thousands(child.itemCount) : ""
            html += "<tr><td class=\"p\">\(name)</td><td class=\"n\">\(size(bytes))</td><td class=\"n\">\(items)</td>"
            html += "<td class=\"n\">\(shareText)%</td><td><div class=\"bar\" style=\"width:\(shareText)%\"></div></td></tr>"
        }
        return html + moreRows(children.count, limit: tableRows, columns: 5) + "</table>"
    }

    private static func files(_ report: ScanReport, metric: SizeMetric) -> String {
        let largest = report.largestFiles(limit: topFiles, by: metric)
        guard !largest.isEmpty else { return "" }
        var html = "<h2>Largest files</h2><table><tr><th class=\"n\">Size</th><th>Path</th></tr>"
        for file in largest {
            html += "<tr><td class=\"n\">\(size(file.bytes(for: metric)))</td><td class=\"p\">\(escape(file.path))</td></tr>"
        }
        return html + "</table>"
    }

    private static func fileTypes(_ root: ScanEntry, metric: SizeMetric) -> String {
        let list = FileTypes.breakdown(of: root, metric: metric)
        guard !list.isEmpty else { return "" }
        var html = "<h2>File types</h2><table><tr><th>Type</th><th class=\"n\">Size</th><th class=\"n\">Files</th></tr>"
        for kind in list.prefix(types) {
            html += "<tr><td>\(escape(kind.label))</td><td class=\"n\">\(size(kind.bytes(for: metric)))</td>"
            html += "<td class=\"n\">\(thousands(kind.fileCount))</td></tr>"
        }
        return html + moreRows(list.count, limit: types, columns: 3) + "</table>"
    }

    /// A last row saying how many rows past `limit` aren't listed.
    private static func moreRows(_ count: Int, limit: Int, columns: Int) -> String {
        count > limit ? "<tr><td colspan=\"\(columns)\">and \(thousands(UInt64(count - limit))) more</td></tr>" : ""
    }

    /// A rectangle in percent of the treemap.
    struct Box: Equatable {
        var x = 0.0, y = 0.0, width = 0.0, height = 0.0
    }

    /// A squarified treemap layout (Bruls, Huizing and van Wijk, 2000), as the CLI's
    /// `treemap::layout` draws it: one rectangle per size, largest first, in the same order.
    static func layout(_ sizes: [Double], width: Double, height: Double) -> [Box] {
        var rects = Array(repeating: Box(), count: sizes.count)
        let total = sizes.reduce(0, +)
        guard total > 0, width > 0, height > 0 else { return rects }
        let scale = width * height / total
        let areas = sizes.map { $0 * scale }
        var free = Box(x: 0, y: 0, width: width, height: height)
        var start = 0
        while start < areas.count, areas[start] > 0 {
            let side = min(free.width, free.height)
            var end = start + 1
            while end < areas.count, areas[end] > 0,
                  worst(areas[start...end], side: side) <= worst(areas[start..<end], side: side) {
                end += 1
            }
            let row = areas[start..<end]
            let sum = row.reduce(0, +)
            if free.width >= free.height {
                // A column along the left edge.
                let columnWidth = free.height > 0 ? sum / free.height : 0
                var y = free.y
                for (offset, part) in row.enumerated() {
                    let partHeight = columnWidth > 0 ? part / columnWidth : 0
                    rects[start + offset] = Box(x: free.x, y: y, width: columnWidth, height: partHeight)
                    y += partHeight
                }
                free.x += columnWidth
                free.width = max(0, free.width - columnWidth)
            } else {
                // A row along the top edge.
                let rowHeight = free.width > 0 ? sum / free.width : 0
                var x = free.x
                for (offset, part) in row.enumerated() {
                    let partWidth = rowHeight > 0 ? part / rowHeight : 0
                    rects[start + offset] = Box(x: x, y: free.y, width: partWidth, height: rowHeight)
                    x += partWidth
                }
                free.y += rowHeight
                free.height = max(0, free.height - rowHeight)
            }
            start = end
        }
        return rects
    }

    /// The worst aspect ratio of a row of `areas` laid along a side of length `side`.
    private static func worst(_ areas: ArraySlice<Double>, side: Double) -> Double {
        let sum = areas.reduce(0, +)
        guard let smallest = areas.min(), let largest = areas.max(), sum > 0, smallest > 0 else {
            return .greatestFiniteMagnitude
        }
        let (side2, sum2) = (side * side, sum * sum)
        return max(side2 * largest / sum2, sum2 / (side2 * smallest))
    }

    /// Escapes text for HTML content and attribute values.
    static func escape(_ text: String) -> String {
        var escaped = ""
        escaped.reserveCapacity(text.utf8.count)
        for character in text {
            switch character {
            case "&": escaped += "&amp;"
            case "<": escaped += "&lt;"
            case ">": escaped += "&gt;"
            case "\"": escaped += "&quot;"
            case "'": escaped += "&#39;"
            default: escaped.append(character)
            }
        }
        return escaped
    }

    private static func size(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }

    private static func thousands(_ value: UInt64) -> String {
        let digits = String(value)
        var grouped = ""
        for (index, digit) in digits.enumerated() {
            if index > 0, (digits.count - index) % 3 == 0 { grouped += "," }
            grouped.append(digit)
        }
        return grouped
    }

    private static func percent(_ value: Double) -> String { String(format: "%.3f", value) }

    private static func stamp(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }
}
