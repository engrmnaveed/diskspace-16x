import Foundation

/// A mounted volume, with the capacity Finder reports for it.
public struct VolumeSummary: Identifiable, Sendable, Equatable {
    public var id: String { path }
    public let name: String
    public let path: String
    public let totalBytes: UInt64
    /// Space available for new files, including purgeable space macOS can free.
    public let availableBytes: UInt64

    public init(name: String, path: String, totalBytes: UInt64, availableBytes: UInt64) {
        self.name = name
        self.path = path
        self.totalBytes = totalBytes
        self.availableBytes = min(availableBytes, totalBytes)
    }

    public var usedBytes: UInt64 { totalBytes - availableBytes }
    public var usedFraction: Double { totalBytes == 0 ? 0 : Double(usedBytes) / Double(totalBytes) }

    #if os(macOS)
    /// The volumes Finder shows, starting with the startup disk. Network volumes can be
    /// slow to answer, so call this off the main actor.
    public static func mounted() -> [VolumeSummary] {
        let keys: [URLResourceKey] = [
            .volumeNameKey, .volumeTotalCapacityKey, .volumeAvailableCapacityForImportantUsageKey,
            .volumeAvailableCapacityKey,
        ]
        let urls = FileManager.default.mountedVolumeURLs(
            includingResourceValuesForKeys: keys, options: [.skipHiddenVolumes]
        ) ?? []
        return urls.compactMap { url in
            guard let values = try? url.resourceValues(forKeys: Set(keys)),
                  let total = values.volumeTotalCapacity, total > 0
            else { return nil }
            let available = values.volumeAvailableCapacityForImportantUsage
                ?? Int64(values.volumeAvailableCapacity ?? 0)
            var path = url.path(percentEncoded: false)
            while path.count > 1, path.hasSuffix("/") { path.removeLast() }
            return VolumeSummary(
                name: values.volumeName ?? url.lastPathComponent, path: path,
                totalBytes: UInt64(total), availableBytes: UInt64(max(0, available))
            )
        }
    }
    #endif
}
