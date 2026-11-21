import Foundation

/// A scan saved in ncdu's JSON export format (https://dev.yorhel.nl/ncdu/jsonfmt), which
/// the Space 16X CLI, ncdu and gdu also read and write. Space 16X adds `space16x_*` keys,
/// which other tools ignore, to keep issues, excludes and entry states.
public struct SavedScan: Sendable {
    public let report: ScanReport
    /// The exclude patterns the scan used, as given.
    public let excludes: [String]

    public init(report: ScanReport, excludes: [String]) {
        self.report = report
        self.excludes = excludes
    }

    /// Why a file can't be opened as a saved scan.
    public struct ReadError: LocalizedError, Equatable {
        public let message: String
        public var errorDescription: String? { message }
    }

    /// Reads an export from ncdu, gdu or Space 16X. Folder totals count their contents, as
    /// a scan does, so a folder's own size in the file isn't added. Excluded, special and
    /// other-filesystem entries are left out, and a file with the same device and inode as
    /// an earlier one is a hard-link alias.
    public init(contentsOf url: URL) throws {
        try self.init(data: Data(contentsOf: url, options: .mappedIfSafe))
    }

    public init(data: Data) throws {
        self = try data.withUnsafeBytes { bytes in
            var reader = ExportReader(bytes: bytes)
            return try reader.read()
        }
    }

    /// The scan as an ncdu JSON export, one entry per line. `version` is the app's.
    public func exportData(version: String) -> Data {
        var writer = ExportWriter()
        writer.write(self, version: version)
        return Data(writer.output)
    }
}

private struct ExportWriter {
    var output: [UInt8] = []

    mutating func write(_ scan: SavedScan, version: String) {
        let report = scan.report
        append("[1,2,{\"progname\":\"space16x\",\"progver\":")
        string(version)
        append(",\"timestamp\":\(UInt64(max(0, Date().timeIntervalSince1970)))")
        append(",\"space16x_excludedCount\":\(report.excludedCount),\"space16x_excludes\":[")
        for (index, pattern) in scan.excludes.enumerated() {
            if index > 0 { append(",") }
            string(pattern)
        }
        append("],\"space16x_issues\":[")
        // An entry with an error reads as incomplete, and one only refused access as protected.
        var issueKinds: [String: ScanIssue.Kind] = [:]
        for (index, issue) in report.issues.enumerated() {
            append(index > 0 ? ",\n{\"path\":" : "\n{\"path\":")
            string(issue.path)
            append(",\"message\":")
            string(issue.message)
            append(",\"kind\":\"\(issue.kind.rawValue)\"}")
            if issueKinds[issue.path] != .error { issueKinds[issue.path] = issue.kind }
        }
        append("]},\n[")
        info(report.root, name: report.root.path, issueKinds: issueKinds)
        // Children of the open folders, so deep trees need no recursion.
        var open = [report.root.children[...]]
        while let level = open.indices.last {
            guard let child = open[level].popFirst() else {
                open.removeLast()
                append("]")
                continue
            }
            if child.isDirectory {
                append(",\n[")
                open.append(child.children[...])
            } else {
                append(",\n")
            }
            info(child, name: child.name, issueKinds: issueKinds)
        }
        append("]\n")
    }

    private mutating func info(_ entry: ScanEntry, name: String, issueKinds: [String: ScanIssue.Kind]) {
        append("{\"name\":")
        string(name)
        if !entry.isDirectory {
            append(",\"asize\":\(entry.logicalBytes),\"dsize\":\(entry.allocatedBytes)")
        }
        let issue = issueKinds.isEmpty ? nil : issueKinds[entry.path]
        if let issue {
            append(issue == .error ? ",\"read_error\":true" : ",\"read_error\":true,\"space16x_state\":\"protected\"")
        } else if entry.isCloudOnly {
            append(",\"space16x_state\":\"cloudOnly\"")
        } else if entry.isHardLinkAlias {
            append(",\"space16x_state\":\"hardLinkAlias\"")
        } else if entry.isPending {
            append(",\"read_error\":true")
        }
        append("}")
    }

    private mutating func append(_ text: String) {
        output.append(contentsOf: text.utf8)
    }

    private mutating func string(_ text: String) {
        output.append(0x22)
        for byte in text.utf8 {
            switch byte {
            case 0x22, 0x5C:
                output.append(0x5C)
                output.append(byte)
            case 0..<0x20:
                output.append(contentsOf: "\\u00".utf8)
                output.append(hexDigit(byte >> 4))
                output.append(hexDigit(byte & 15))
            default:
                output.append(byte)
            }
        }
        output.append(0x22)
    }

    private func hexDigit(_ value: UInt8) -> UInt8 {
        value < 10 ? 0x30 + value : 0x57 + value
    }
}

/// A folder being read: its contents so far and their totals.
private struct ImportFrame {
    let name: String
    let container: ScanLocation?
    let location: ScanLocation
    let isCloudOnly: Bool
    let device: UInt64
    var children: [ScanEntry] = []
    var logical: UInt64 = 0
    var allocated: UInt64 = 0
    var items: UInt64 = 0

    mutating func add(_ entry: ScanEntry) {
        children.append(entry)
        logical = logical.clampedAdding(entry.logicalBytes)
        allocated = allocated.clampedAdding(entry.allocatedBytes)
        if !entry.isHardLinkAlias {
            items = items.clampedAdding(entry.itemCount.clampedAdding(1))
        }
    }

    func entry() -> ScanEntry {
        ScanEntry(
            name: name, container: container, isDirectory: true, isHardLinkAlias: false,
            isCloudOnly: isCloudOnly, isPending: false, logicalBytes: logical,
            allocatedBytes: allocated, itemCount: items, children: children
        )
    }
}

private extension UInt64 {
    func clampedAdding(_ other: UInt64) -> UInt64 {
        let (sum, overflow) = addingReportingOverflow(other)
        return overflow ? .max : sum
    }
}

private struct ExportInfo {
    var name: [UInt8] = []
    var apparent: UInt64 = 0
    var allocated: UInt64 = 0
    var device: UInt64?
    var inode: UInt64 = 0
    var links: UInt64 = 1
    var isHardLink = false
    var readError = false
    var isSpecial = false
    var excluded: [UInt8]?
    var state: String?
}

private struct LinkIdentity: Hashable {
    let device: UInt64
    let inode: UInt64
}

/// Reads JSON bytes directly. Strings may hold bytes that aren't UTF-8, because ncdu
/// writes names as the filesystem has them, and those decode with replacement characters.
private struct ExportReader {
    let bytes: UnsafeRawBufferPointer
    var index = 0
    var line = 1

    init(bytes: UnsafeRawBufferPointer) {
        self.bytes = bytes
    }

    mutating func read() throws -> SavedScan {
        skipSpace()
        switch peek() {
        case 0x5B: break
        case 0xBF:
            throw SavedScan.ReadError(message: "This is an ncdu binary export. Export JSON from ncdu with -o instead of -O.")
        default:
            throw SavedScan.ReadError(message: "This isn’t an ncdu JSON export.")
        }
        index += 1
        let major = try number()
        guard major == 1 else {
            throw SavedScan.ReadError(message: "This ncdu export uses format version \(major), which Space 16X can’t read.")
        }
        try expect(0x2C)
        _ = try number()
        try expect(0x2C)

        var excludes: [String] = []
        var excludedCount: UInt64?
        var savedIssues: [ScanIssue]?
        try object { reader, key in
            switch key {
            case "space16x_excludedCount":
                excludedCount = try reader.number()
            case "space16x_excludes":
                try reader.array { reader in excludes.append(try reader.text()) }
            case "space16x_issues":
                var list: [ScanIssue] = []
                try reader.array { reader in list.append(try reader.issue()) }
                savedIssues = list
            default:
                try reader.skipValue()
            }
        }
        try expect(0x2C)
        try expect(0x5B)
        let rootInfo = try info()
        var rootPath = String(decoding: rootInfo.name, as: UTF8.self)
        while rootPath.count > 1, rootPath.hasSuffix("/") { rootPath.removeLast() }
        guard !rootPath.isEmpty else { throw error("the scan root has no name") }

        // Space 16X exports list every issue, so only other tools' read errors become issues.
        let issuesFromEntries = savedIssues == nil
        var issues = savedIssues ?? []
        if issuesFromEntries, rootInfo.readError { issues.append(Self.readError(rootPath)) }
        let rootEntry = ScanEntry(
            path: rootPath, isDirectory: true, isHardLinkAlias: false, isCloudOnly: false,
            isPending: false, logicalBytes: 0, allocatedBytes: 0, itemCount: 0, children: []
        )
        var stack = [ImportFrame(
            name: rootEntry.name, container: rootEntry.container,
            location: ScanLocation(path: rootPath),
            isCloudOnly: rootInfo.state == "cloudOnly", device: rootInfo.device ?? 0
        )]
        var links = Set<LinkIdentity>()
        var patternMatches: UInt64 = 0
        while true {
            switch try token() {
            case 0x2C:
                break
            case 0x5D where stack.count == 1:
                try expect(0x5D)
                skipSpace()
                guard index == bytes.count else { throw error("unexpected data after the export") }
                return SavedScan(
                    report: ScanReport(
                        root: stack[0].entry(), issues: issues,
                        excludedCount: excludedCount ?? patternMatches
                    ),
                    excludes: excludes
                )
            case 0x5D:
                let folder = stack.removeLast().entry()
                stack[stack.count - 1].add(folder)
                continue
            default:
                throw error("expected ',' or ']'")
            }
            skipSpace()
            let isFolder = peek() == 0x5B
            if isFolder { index += 1 }
            let entry = try info()
            guard !entry.name.isEmpty, entry.name != [0x2E], entry.name != [0x2E, 0x2E],
                  !entry.name.contains(0x2F)
            else { throw error("an entry has an invalid name") }
            if entry.excluded != nil || entry.isSpecial {
                // Patterns count as exclusions. Other filesystems and special files are
                // skipped quietly, as a scan skips them.
                if let kind = entry.excluded, !Self.otherFilesystems.contains(kind) { patternMatches += 1 }
                if isFolder { try skipFolder() }
                continue
            }
            let parent = stack[stack.count - 1]
            let name = String(decoding: entry.name, as: UTF8.self)
            let device = entry.device ?? parent.device
            if issuesFromEntries, entry.readError {
                issues.append(Self.readError(parent.location.path(of: name)))
            }
            if isFolder {
                stack.append(ImportFrame(
                    name: name, container: parent.location,
                    location: ScanLocation(path: parent.location.path(of: name)),
                    isCloudOnly: entry.state == "cloudOnly", device: device
                ))
                continue
            }
            let isAlias = entry.state == "hardLinkAlias"
                || ((entry.isHardLink || entry.links > 1)
                    && !links.insert(LinkIdentity(device: device, inode: entry.inode)).inserted)
            stack[stack.count - 1].add(ScanEntry(
                name: name, container: parent.location, isDirectory: false,
                isHardLinkAlias: isAlias, isCloudOnly: entry.state == "cloudOnly", isPending: false,
                logicalBytes: isAlias ? 0 : entry.apparent, allocatedBytes: isAlias ? 0 : entry.allocated,
                itemCount: 0, children: []
            ))
        }
    }

    private static func readError(_ path: String) -> ScanIssue {
        ScanIssue(path: path, message: "It couldn’t be read when the scan was made.")
    }

    private func error(_ message: String) -> SavedScan.ReadError {
        SavedScan.ReadError(message: "Line \(line): \(message).")
    }

    private func peek() -> UInt8? {
        index < bytes.count ? bytes[index] : nil
    }

    private mutating func next() throws -> UInt8 {
        guard let byte = peek() else { throw error("the export ends early") }
        index += 1
        if byte == 0x0A { line += 1 }
        return byte
    }

    private mutating func skipSpace() {
        while let byte = peek(), byte == 0x20 || byte == 0x09 || byte == 0x0A || byte == 0x0D {
            index += 1
            if byte == 0x0A { line += 1 }
        }
    }

    /// The next byte after any whitespace.
    private mutating func token() throws -> UInt8 {
        skipSpace()
        return try next()
    }

    private mutating func expect(_ byte: UInt8) throws {
        guard try token() == byte else { throw error("expected '\(Character(Unicode.Scalar(byte)))'") }
    }

    private mutating func number() throws -> UInt64 {
        skipSpace()
        var value: UInt64 = 0
        var digits = 0
        while let byte = peek(), (0x30...0x39).contains(byte) {
            index += 1
            digits += 1
            let (scaled, overflow) = value.multipliedReportingOverflow(by: 10)
            let (sum, carry) = scaled.addingReportingOverflow(UInt64(byte - 0x30))
            guard !overflow, !carry else { throw error("a number is too large") }
            value = sum
        }
        guard digits > 0, peek() != 0x2E, peek() != 0x65, peek() != 0x45 else {
            throw error("expected a whole number")
        }
        return value
    }

    private mutating func boolean() throws -> Bool {
        let first = try token()
        while let byte = peek(), (0x61...0x7A).contains(byte) { index += 1 }
        switch first {
        case 0x74: return true
        case 0x66, 0x6E: return false
        default: throw error("expected true or false")
        }
    }

    private mutating func string() throws -> [UInt8] {
        try expect(0x22)
        return try restOfString()
    }

    private mutating func text() throws -> String {
        String(decoding: try string(), as: UTF8.self)
    }

    /// The rest of a string after its opening quote, unescaped.
    private mutating func restOfString() throws -> [UInt8] {
        var result: [UInt8] = []
        while true {
            let byte = try next()
            switch byte {
            case 0x22:
                return result
            case 0x5C:
                switch try next() {
                case 0x62: result.append(0x08)
                case 0x66: result.append(0x0C)
                case 0x6E: result.append(0x0A)
                case 0x72: result.append(0x0D)
                case 0x74: result.append(0x09)
                case 0x75:
                    var code = try hex4()
                    if (0xD800..<0xDC00).contains(code), peek() == 0x5C {
                        index += 1
                        guard try next() == 0x75 else { throw error("invalid escape") }
                        let low = try hex4()
                        if (0xDC00..<0xE000).contains(low) {
                            code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                        } else {
                            result.append(contentsOf: "\u{FFFD}".utf8)
                            code = low
                        }
                    }
                    let scalar = Unicode.Scalar(code) ?? "\u{FFFD}"
                    result.append(contentsOf: String(Character(scalar)).utf8)
                case let other where other == 0x22 || other == 0x5C || other == 0x2F:
                    result.append(other)
                default:
                    throw error("invalid escape")
                }
            default:
                result.append(byte)
            }
        }
    }

    private mutating func hex4() throws -> UInt32 {
        var code: UInt32 = 0
        for _ in 0..<4 {
            guard let digit = Character(Unicode.Scalar(try next())).hexDigitValue else {
                throw error("invalid escape")
            }
            code = code * 16 + UInt32(digit)
        }
        return code
    }

    /// Skips any value. Commas and colons between values inside it aren't checked.
    private mutating func skipValue() throws {
        var depth = 0
        repeat {
            switch try token() {
            case 0x22:
                _ = try restOfString()
            case 0x5B, 0x7B:
                depth += 1
            case 0x5D, 0x7D:
                guard depth > 0 else { throw error("unexpected ']' or '}'") }
                depth -= 1
            default:
                while let byte = peek(), !Self.endsScalar(byte) { index += 1 }
            }
        } while depth > 0
    }

    private static func endsScalar(_ byte: UInt8) -> Bool {
        switch byte {
        case 0x2C, 0x3A, 0x5B, 0x5D, 0x7B, 0x7D, 0x22, 0x20, 0x09, 0x0D, 0x0A: true
        default: false
        }
    }

    /// Exclusion kinds for other filesystems, which a scan skips without counting.
    private static let otherFilesystems: [[UInt8]] = ["otherfs", "othfs", "kernfs", "frmlnk", "frmlink"]
        .map { Array($0.utf8) }

    /// Skips the rest of a folder's array after its info object.
    private mutating func skipFolder() throws {
        while true {
            switch try token() {
            case 0x2C: try skipValue()
            case 0x5D: return
            default: throw error("expected ',' or ']'")
            }
        }
    }

    private mutating func array(_ item: (inout ExportReader) throws -> Void) throws {
        try expect(0x5B)
        skipSpace()
        if peek() == 0x5D {
            index += 1
            return
        }
        while true {
            try item(&self)
            switch try token() {
            case 0x2C: continue
            case 0x5D: return
            default: throw error("expected ',' or ']'")
            }
        }
    }

    private mutating func object(_ field: (inout ExportReader, String) throws -> Void) throws {
        try expect(0x7B)
        skipSpace()
        if peek() == 0x7D {
            index += 1
            return
        }
        while true {
            let key = try text()
            try expect(0x3A)
            try field(&self, key)
            switch try token() {
            case 0x2C: continue
            case 0x7D: return
            default: throw error("expected ',' or '}'")
            }
        }
    }

    private mutating func info() throws -> ExportInfo {
        var info = ExportInfo()
        try object { reader, key in
            switch key {
            case "name": info.name = try reader.string()
            case "asize": info.apparent = try reader.number()
            case "dsize": info.allocated = try reader.number()
            case "dev": info.device = try reader.number()
            case "ino": info.inode = try reader.number()
            case "nlink": info.links = try reader.number()
            case "hlnkc": info.isHardLink = try reader.boolean()
            case "read_error": info.readError = try reader.boolean()
            case "notreg": info.isSpecial = try reader.boolean()
            case "excluded": info.excluded = try reader.string()
            case "space16x_state": info.state = try reader.text()
            default: try reader.skipValue()
            }
        }
        return info
    }

    private mutating func issue() throws -> ScanIssue {
        var path = ""
        var message = ""
        var kind = ScanIssue.Kind.error
        try object { reader, key in
            switch key {
            case "path": path = try reader.text()
            case "message": message = try reader.text()
            case "kind": kind = ScanIssue.Kind(rawValue: try reader.text()) ?? .error
            default: try reader.skipValue()
            }
        }
        return ScanIssue(path: path, message: message, kind: kind)
    }
}
