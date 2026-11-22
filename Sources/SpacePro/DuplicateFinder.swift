import Foundation
import SpaceCore

/// Files with the same contents, in path order.
public struct DuplicateGroup: Identifiable, Sendable {
    public var id: String { files[0].path }
    /// The file length each copy has.
    public let length: UInt64
    public let files: [ScanEntry]
    /// The bytes of each file that moving it alone to the Trash wouldn't free, because it
    /// shares them with an APFS clone or has another hard link. Zero for most.
    public let sharedBytes: [UInt64]

    public init(length: UInt64, files: [ScanEntry], sharedBytes: [UInt64]? = nil) {
        self.length = length
        self.files = files
        self.sharedBytes = sharedBytes?.count == files.count ? sharedBytes! : Array(repeating: 0, count: files.count)
    }

    /// Whether the copy at `index` shares blocks with another file, such as an APFS clone,
    /// so that removing it alone frees only its own blocks.
    public func isClone(at index: Int) -> Bool {
        sharedBytes[index] > 0
    }

    /// The bytes that moving the copy at `index` alone to the Trash frees at least.
    public func exclusiveBytes(at index: Int) -> UInt64 {
        let allocated = files[index].allocatedBytes
        return allocated - min(allocated, sharedBytes[index])
    }

    /// The space that moving every copy but one to the Trash and emptying it frees at least,
    /// as `space16x duplicates` counts it. Each copy frees its exclusive bytes. The copy kept
    /// is the largest, or, when some copies share blocks, the clone that would free the
    /// most, since its blocks may be the ones the other clones share.
    public var extraBytes: UInt64 {
        let anyClone = files.indices.contains { isClone(at: $0) }
        var total: UInt64 = 0
        var kept: UInt64 = 0
        for index in files.indices {
            let bytes = exclusiveBytes(at: index)
            total += bytes
            if !anyClone || isClone(at: index) { kept = max(kept, bytes) }
        }
        return total - kept
    }

    /// The group with only the copies `isKept` accepts, each with its shared bytes, or nil
    /// when fewer than two remain. A clone moved to the Trash still shares its blocks until
    /// the Trash is emptied, so the remaining copies keep their measurements.
    public func keeping(_ isKept: (ScanEntry) -> Bool) -> DuplicateGroup? {
        let kept = files.indices.filter { isKept(files[$0]) }
        guard kept.count > 1 else { return nil }
        return DuplicateGroup(length: length, files: kept.map { files[$0] }, sharedBytes: kept.map { sharedBytes[$0] })
    }
}

/// Finds duplicate files in a finished scan, as `space16x duplicates` does. It is the one
/// feature that reads file contents, and only when the user asks for it: it groups files
/// by length and reads only files that share a length with another, first their first
/// 64 KB, then whole files whose starts match. Contents are compared by `Hasher`, Swift's
/// SipHash with a random seed for each launch.
public enum DuplicateFinder {
    public static let defaultMinimumBytes: UInt64 = 1_000_000
    static let headBytes = 64 * 1024

    /// Groups the scan's files of at least `minimumBytes` by contents, most extra space
    /// first, and the paths of files that couldn't be read. Returns no groups when cancelled.
    public static func find(
        in report: ScanReport, minimumBytes: UInt64 = defaultMinimumBytes
    ) -> (groups: [DuplicateGroup], unreadable: [String]) {
        var byLength: [UInt64: [ScanEntry]] = [:]
        var folders = [report.root]
        while let folder = folders.popLast() {
            for child in folder.children {
                if child.isDirectory {
                    folders.append(child)
                } else if !child.isHardLinkAlias, !child.isCloudOnly, !child.isPending,
                          child.logicalBytes >= max(minimumBytes, 1) {
                    byLength[child.logicalBytes, default: []].append(child)
                }
            }
        }
        var groups: [DuplicateGroup] = []
        var unreadable: [String] = []
        func digest(_ entry: ScanEntry, limit: Int) -> Int? {
            if let hash = hash(atPath: entry.path, limit: limit) { return hash }
            unreadable.append(entry.path)
            return nil
        }
        for (length, files) in byLength where files.count > 1 {
            if Task.isCancelled { return ([], []) }
            for heads in split(files, by: { digest($0, limit: headBytes) }) {
                if length <= UInt64(headBytes) {
                    groups.append(DuplicateGroup(length: length, files: heads))
                    continue
                }
                for whole in split(heads, by: { digest($0, limit: .max) }) {
                    groups.append(DuplicateGroup(length: length, files: whole))
                }
            }
        }
        // One metadata call per copy finds the ones that share blocks.
        let sorted = groups.map { group in
            let files = group.files.sorted { $0.path.utf8.lexicographicallyPrecedes($1.path.utf8) }
            let shared = files.map { min(ExclusiveSize.sharedBytes(atPath: $0.path), $0.allocatedBytes) }
            return DuplicateGroup(length: group.length, files: files, sharedBytes: shared)
        }.sorted { left, right in
            left.extraBytes == right.extraBytes
                ? left.files[0].path.utf8.lexicographicallyPrecedes(right.files[0].path.utf8)
                : left.extraBytes > right.extraBytes
        }
        return (sorted, unreadable.sorted())
    }

    /// Groups `files` by `key`, leaving out files without one and groups of one.
    private static func split(_ files: [ScanEntry], by key: (ScanEntry) -> Int?) -> [[ScanEntry]] {
        var groups: [Int: [ScanEntry]] = [:]
        for file in files {
            if let hash = key(file) { groups[hash, default: []].append(file) }
        }
        return groups.values.filter { $0.count > 1 }
    }

    /// A hash of the first `limit` bytes of the file and the length read, or nil when it
    /// can't be read.
    static func hash(atPath path: String, limit: Int) -> Int? {
        guard let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        var hasher = Hasher()
        var remaining = limit
        var read = 0
        while remaining > 0 {
            if Task.isCancelled { return nil }
            let chunk: Data?
            do { chunk = try handle.read(upToCount: min(remaining, 256 * 1024)) } catch { return nil }
            // A read at the end of the file returns nil or no data.
            guard let chunk, !chunk.isEmpty else { break }
            chunk.withUnsafeBytes { hasher.combine(bytes: $0) }
            read += chunk.count
            remaining -= chunk.count
        }
        hasher.combine(read)
        return hasher.finalize()
    }
}
