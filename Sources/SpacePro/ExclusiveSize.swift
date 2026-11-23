import Foundation
import SpaceCore

/// Exclusive size, as `diskuse16x scan --exclusive` measures it: the space that moving items
/// to the Trash and emptying it frees at least. Blocks a file shares with APFS clones
/// don't count, and neither does a file with other hard links. It reads metadata only.
public enum ExclusiveSize {
    /// The bytes that removing `entries` frees at least, or nil when measuring takes longer
    /// than `limit` or the task is cancelled. Files that can't be read count as unshared.
    public static func measure(_ entries: [ScanEntry], limit: Duration = .seconds(2)) -> UInt64? {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: limit)
        var total: UInt64 = 0
        var pending = entries[...]
        var visited = 0
        while let entry = pending.popFirst() {
            if visited.isMultiple(of: 256), Task.isCancelled || clock.now >= deadline { return nil }
            visited += 1
            if entry.isDirectory {
                pending.append(contentsOf: entry.children)
            } else if !entry.isHardLinkAlias, !entry.isCloudOnly, entry.allocatedBytes > 0 {
                let shared = min(sharedBytes(atPath: entry.path), entry.allocatedBytes)
                total += entry.allocatedBytes - shared
            }
        }
        return total
    }

    /// The bytes of the file at `path` that removing it wouldn't free: all of them when it
    /// has other hard links, else the blocks it shares with clones. Zero when unknown.
    public static func sharedBytes(atPath path: String) -> UInt64 {
        var request = attrlist(
            bitmapcount: u_short(ATTR_BIT_MAP_COUNT), reserved: 0,
            commonattr: attrgroup_t(ATTR_CMN_RETURNED_ATTRS), volattr: 0, dirattr: 0,
            fileattr: attrgroup_t(ATTR_FILE_LINKCOUNT) | attrgroup_t(ATTR_FILE_ALLOCSIZE),
            forkattr: attrgroup_t(ATTR_CMNEXT_PRIVATESIZE)
        )
        // The length, the returned attributes, the link count, the allocated size and the
        // private size, each packed at 4-byte alignment.
        var buffer = [UInt8](repeating: 0, count: 48)
        let options = UInt32(FSOPT_NOFOLLOW) | UInt32(FSOPT_ATTR_CMN_EXTENDED) | UInt32(FSOPT_PACK_INVAL_ATTRS)
        guard getattrlist(path, &request, &buffer, buffer.count, options) == 0 else { return 0 }
        return buffer.withUnsafeBytes { bytes in
            let returnedFork = bytes.loadUnaligned(fromByteOffset: 4 + 16, as: UInt32.self)
            let links = bytes.loadUnaligned(fromByteOffset: 24, as: UInt32.self)
            let allocated = UInt64(max(0, bytes.loadUnaligned(fromByteOffset: 28, as: Int64.self)))
            if links > 1 { return allocated }
            guard returnedFork & UInt32(ATTR_CMNEXT_PRIVATESIZE) != 0 else { return 0 }
            let privateBytes = UInt64(max(0, bytes.loadUnaligned(fromByteOffset: 36, as: Int64.self)))
            return allocated - min(allocated, privateBytes)
        }
    }
}

/// Exclusive sizes of a whole scan for the Exclusive column, measured after the scan and
/// only when the user asks, as the CLI explorer's Exclusive column measures them. Only
/// files that share blocks and the folders above them are kept.
public struct ExclusiveSizes: Sendable {
    /// Shared bytes of each file that shares blocks, and the totals of the folders above,
    /// by path.
    let shared: [String: UInt64]

    /// The bytes that moving `entry` to the Trash and emptying it frees at least.
    public func bytes(for entry: ScanEntry) -> UInt64 {
        entry.allocatedBytes - min(entry.allocatedBytes, shared[entry.path] ?? 0)
    }

    /// Measures every counted file of `report`, `concurrency` files at a time, and reports
    /// the share done so far, from 0 to 1. Returns nil when the task is cancelled. Files
    /// that can't be read count as unshared.
    public static func measure(
        _ report: ScanReport, concurrency: Int = 8, progress: @escaping @Sendable (Double) -> Void = { _ in }
    ) async -> ExclusiveSizes? {
        var files: [ScanEntry] = []
        var folders = [report.root]
        while let folder = folders.popLast() {
            for child in folder.children {
                if child.isDirectory {
                    folders.append(child)
                } else if !child.isHardLinkAlias, !child.isCloudOnly, !child.isPending, child.allocatedBytes > 0 {
                    files.append(child)
                }
            }
        }
        let size = 2048
        let chunks = stride(from: 0, to: files.count, by: size).map { files[$0..<min($0 + size, files.count)] }
        var sharing: [(path: String, bytes: UInt64)] = []
        var done = 0
        await withTaskGroup(of: [(path: String, bytes: UInt64)].self) { group in
            var next = 0
            func addNext() {
                guard next < chunks.count else { return }
                let chunk = chunks[next]
                next += 1
                group.addTask {
                    var found: [(path: String, bytes: UInt64)] = []
                    for file in chunk {
                        if Task.isCancelled { break }
                        let path = file.path
                        let bytes = min(ExclusiveSize.sharedBytes(atPath: path), file.allocatedBytes)
                        if bytes > 0 { found.append((path, bytes)) }
                    }
                    return found
                }
            }
            for _ in 0..<max(1, concurrency) { addNext() }
            for await found in group {
                sharing += found
                done += 1
                progress(Double(done) / Double(chunks.count))
                addNext()
            }
        }
        if Task.isCancelled { return nil }
        let root = report.root.path
        var shared: [String: UInt64] = [:]
        for (path, bytes) in sharing {
            var current = path
            while true {
                shared[current, default: 0] += bytes
                guard current.count > root.count else { break }
                current = (current as NSString).deletingLastPathComponent
            }
        }
        return ExclusiveSizes(shared: shared)
    }
}
