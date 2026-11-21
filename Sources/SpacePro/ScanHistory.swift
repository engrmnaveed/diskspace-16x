import Foundation
import SpaceCore

/// One kept scan, as the history's `index.jsonl` records it. The CLI's
/// `space16x history` writes the same records.
public struct SavedScanRecord: Codable, Sendable, Identifiable, Equatable {
    /// The export's file name in the history folder.
    public let file: String
    public let scope: String
    /// When the scan was saved, in Unix seconds.
    public let savedAt: UInt64
    public let allocatedBytes: UInt64
    public let apparentBytes: UInt64
    public let itemCount: UInt64
    public let isPartial: Bool

    public var id: String { file }
    public var date: Date { Date(timeIntervalSince1970: TimeInterval(savedAt)) }
}

/// Saved scans: exports the user chose to keep, in ncdu's JSON format, with an index that
/// lists them without reading each one. Nothing is written except by `save`.
public struct ScanHistory: Sendable {
    static let indexName = "index.jsonl"

    public let directory: URL

    public init(directory: URL) {
        self.directory = directory
    }

    /// `~/Library/Application Support/Space 16X/History`.
    public static var standard: ScanHistory {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return ScanHistory(directory: support.appendingPathComponent("Space 16X/History", isDirectory: true))
    }

    /// The export of `record`.
    public func url(for record: SavedScanRecord) -> URL {
        directory.appendingPathComponent(record.file)
    }

    /// The kept scans of `scope`, or of every folder, newest first. Records whose file is
    /// gone, or lines that can't be read, are skipped.
    public func records(of scope: String? = nil) -> [SavedScanRecord] {
        guard let text = try? String(contentsOf: directory.appendingPathComponent(Self.indexName), encoding: .utf8) else {
            return []
        }
        let decoder = JSONDecoder()
        let records = text.split(separator: "\n").compactMap { line -> SavedScanRecord? in
            guard let record = try? decoder.decode(SavedScanRecord.self, from: Data(line.utf8)),
                  !record.file.contains("/"), !record.file.hasPrefix("."),
                  scope == nil || record.scope == scope,
                  FileManager.default.fileExists(atPath: url(for: record).path) else { return nil }
            return record
        }
        return records.sorted { $0.savedAt > $1.savedAt }
    }

    /// Writes `saved` to the history folder, readable only by this user, and records it.
    @discardableResult
    public func save(_ saved: SavedScan, version: String, at date: Date = Date()) throws -> SavedScanRecord {
        let manager = FileManager.default
        try manager.createDirectory(
            at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700]
        )
        let time = UInt64(max(0, date.timeIntervalSince1970))
        let root = saved.report.root
        let base = Self.fileName(scope: root.path, time: time)
        var attempt = 1
        var file = base + ".json"
        while manager.fileExists(atPath: directory.appendingPathComponent(file).path) {
            attempt += 1
            file = "\(base)-\(attempt).json"
        }
        let url = directory.appendingPathComponent(file)
        try saved.exportData(version: version).write(to: url, options: .atomic)
        try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        let record = SavedScanRecord(
            file: file, scope: root.path, savedAt: time, allocatedBytes: root.allocatedBytes,
            apparentBytes: root.logicalBytes, itemCount: root.itemCount, isPartial: saved.report.isPartial
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        var line = try encoder.encode(record)
        line.append(0x0A)
        let index = directory.appendingPathComponent(Self.indexName)
        if !manager.fileExists(atPath: index.path) {
            manager.createFile(atPath: index.path, contents: nil, attributes: [.posixPermissions: 0o600])
        }
        let handle = try FileHandle(forWritingTo: index)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: line)
        return record
    }

    /// A file name for a scan of `scope` saved at `time`, as the CLI names them: the UTC
    /// time, then the folder's name in safe characters, as in `20261121T161500Z-srv`.
    static func fileName(scope: String, time: UInt64) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let parts = calendar.dateComponents(
            [.year, .month, .day, .hour, .minute, .second], from: Date(timeIntervalSince1970: TimeInterval(time))
        )
        let stamp = String(
            format: "%04d%02d%02dT%02d%02d%02dZ", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0,
            parts.hour ?? 0, parts.minute ?? 0, parts.second ?? 0
        )
        let name = scope == "/" ? "" : String(scope.split(separator: "/").last ?? "")
        let safe = String(name.utf8.prefix(48).map { byte -> Character in
            let character = Character(UnicodeScalar(byte))
            return byte < 0x80 && (character.isLetter || character.isNumber || "._-".contains(character))
                ? character : "_"
        })
        let trimmed = safe.drop { $0 == "." }
        return stamp + "-" + (trimmed.isEmpty ? "root" : String(trimmed))
    }
}
