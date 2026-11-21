import Foundation
import SpaceCore

/// The files of one type below a folder.
public struct FileTypeTotal: Identifiable, Sendable, Equatable {
    /// The extension without its dot, or empty for files without one.
    public let name: String
    public var allocatedBytes: UInt64 = 0
    public var logicalBytes: UInt64 = 0
    public var fileCount: UInt64 = 0

    public var id: String { name }

    /// The type as lists show it: `.mov`, or “No extension”.
    public var label: String { name.isEmpty ? "No extension" : "." + name }

    public func bytes(for metric: SizeMetric) -> UInt64 {
        switch metric {
        case .allocated: allocatedBytes
        case .logical: logicalBytes
        }
    }
}

/// File types: the space each file extension uses, as in `space16x scan --types N` and
/// the CLI explorer's `t` view. Types come from names alone.
public enum FileTypes {
    /// The longest extension that counts as a type. Longer ones are usually part of a name.
    static let longestExtension = 16

    /// The type of a file named `name`: the text after its last dot in ASCII lowercase, or
    /// an empty string when it has none. A leading dot, as in `.bashrc`, starts a name, and
    /// an extension can't be empty, contain a space, or be longer than 16 bytes.
    /// `fixtures/file-types.json` holds the cases both apps test.
    public static func type(of name: String) -> String {
        let bytes = Array(name.utf8)
        guard let dot = bytes.lastIndex(of: 0x2E), dot > 0 else { return "" }
        let extensionBytes = bytes[(dot + 1)...]
        guard !extensionBytes.isEmpty, extensionBytes.count <= longestExtension,
              !extensionBytes.contains(0x20) else { return "" }
        let lowered = extensionBytes.map { (0x41...0x5A).contains($0) ? $0 + 0x20 : $0 }
        return String(decoding: lowered, as: UTF8.self)
    }

    /// The types of the files below `folder`, largest first by `metric`, with ties in
    /// name order. Each hard-linked file counts once, at the path the scan counted.
    public static func breakdown(of folder: ScanEntry, metric: SizeMetric) -> [FileTypeTotal] {
        var totals: [String: FileTypeTotal] = [:]
        var levels = [folder.children[...]]
        while let level = levels.indices.last {
            if Task.isCancelled { return [] }
            guard let child = levels[level].popFirst() else {
                levels.removeLast()
                continue
            }
            if child.isDirectory {
                levels.append(child.children[...])
                continue
            }
            guard !child.isHardLinkAlias else { continue }
            let name = type(of: child.name)
            var total = totals[name] ?? FileTypeTotal(name: name)
            total.allocatedBytes = total.allocatedBytes.saturatingAdding(child.allocatedBytes)
            total.logicalBytes = total.logicalBytes.saturatingAdding(child.logicalBytes)
            total.fileCount += 1
            totals[name] = total
        }
        return totals.values.sorted {
            let (left, right) = ($0.bytes(for: metric), $1.bytes(for: metric))
            return left == right ? $0.name.utf8.lexicographicallyPrecedes($1.name.utf8) : left > right
        }
    }
}

private extension UInt64 {
    func saturatingAdding(_ amount: UInt64) -> UInt64 {
        let (sum, overflow) = addingReportingOverflow(amount)
        return overflow ? .max : sum
    }
}
