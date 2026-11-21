import Foundation
import SpaceCore

/// Exclusive size, as `space16x scan --exclusive` measures it: the space that moving items
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
