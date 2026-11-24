import Foundation

/// The space on a volume that a scan can't show, as `diskuse16x explain` reports it: the
/// other volumes of its APFS container, purgeable space and local snapshots. It comes
/// from system queries and metadata only, and reads no file contents.
public struct DiskExplanation: Identifiable, Sendable, Equatable {
    public struct ContainerVolume: Identifiable, Sendable, Equatable {
        public var id: String { path }
        public let path: String
        public let usedBytes: UInt64

        public init(path: String, usedBytes: UInt64) {
            self.path = path
            self.usedBytes = usedBytes
        }
    }

    public var id: String { mountPoint }
    public let mountPoint: String
    public let name: String
    public let fileSystem: String
    public let device: String
    public let totalBytes: UInt64
    /// On APFS, the whole container's used space, which its volumes share.
    public let usedBytes: UInt64
    public let availableBytes: UInt64
    /// Space macOS frees when it's needed, or nil when macOS doesn't say.
    public let purgeableBytes: UInt64?
    /// Local snapshot names, or nil when they couldn't be listed.
    public let snapshots: [String]?
    /// The mounted volumes of the APFS container, largest first.
    public let containerVolumes: [ContainerVolume]

    public init(
        mountPoint: String, name: String, fileSystem: String, device: String,
        totalBytes: UInt64, usedBytes: UInt64, availableBytes: UInt64,
        purgeableBytes: UInt64?, snapshots: [String]?, containerVolumes: [ContainerVolume]
    ) {
        self.mountPoint = mountPoint
        self.name = name
        self.fileSystem = fileSystem
        self.device = device
        self.totalBytes = totalBytes
        self.usedBytes = usedBytes
        self.availableBytes = availableBytes
        self.purgeableBytes = purgeableBytes
        self.snapshots = snapshots
        self.containerVolumes = containerVolumes
    }

    /// The local dates of the Time Machine snapshots, oldest first.
    public var snapshotDates: [String] {
        (snapshots ?? []).compactMap(Self.snapshotDate).sorted()
    }

    /// Snapshot names from `tmutil listlocalsnapshots`, which starts with a heading line.
    public static func parseSnapshots(_ output: String) -> [String] {
        output.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && !$0.hasSuffix(":") }
    }

    /// The date in a Time Machine snapshot's name, as `2026-11-21 10:15` for
    /// `com.apple.TimeMachine.2026-11-21-101500.local`.
    public static func snapshotDate(_ name: String) -> String? {
        let prefix = "com.apple.TimeMachine.", suffix = ".local"
        guard name.hasPrefix(prefix), name.hasSuffix(suffix) else { return nil }
        let stamp = Array(name.dropFirst(prefix.count).dropLast(suffix.count).utf8)
        guard stamp.count == 17 else { return nil }
        for (index, byte) in stamp.enumerated() {
            let valid = [4, 7, 10].contains(index) ? byte == UInt8(ascii: "-") : (48...57).contains(byte)
            guard valid else { return nil }
        }
        let text = String(decoding: stamp, as: UTF8.self)
        let characters = Array(text)
        return "\(String(characters[0..<10])) \(String(characters[11..<13])):\(String(characters[13..<15]))"
    }

    /// The APFS container of a volume's device, as `disk3` for `/dev/disk3s5s1`.
    public static func container(ofDevice device: String) -> String? {
        guard device.hasPrefix("/dev/disk") else { return nil }
        let digits = device.dropFirst("/dev/disk".count).prefix(while: \.isNumber)
        return digits.isEmpty ? nil : "disk\(digits)"
    }

    #if os(macOS)
    /// Explains the volume that holds `url`. It runs `tmutil` and can wait on slow
    /// volumes, so call it off the main actor.
    public static func explain(_ url: URL) -> DiskExplanation? {
        var stats = statfs()
        guard statfs(url.path(percentEncoded: false), &stats) == 0 else { return nil }
        let mountPoint = text(stats.f_mntonname)
        let device = text(stats.f_mntfromname)
        let fileSystem = text(stats.f_fstypename)
        let block = UInt64(stats.f_bsize)
        let available = stats.f_bavail * block
        let volumeURL = URL(filePath: mountPoint, directoryHint: .isDirectory)
        let values = try? volumeURL.resourceValues(forKeys: [
            .volumeNameKey, .volumeAvailableCapacityForImportantUsageKey,
        ])
        let important = values?.volumeAvailableCapacityForImportantUsage ?? 0
        return DiskExplanation(
            mountPoint: mountPoint,
            name: values?.volumeName ?? volumeURL.lastPathComponent,
            fileSystem: fileSystem,
            device: device,
            totalBytes: stats.f_blocks * block,
            usedBytes: (stats.f_blocks - min(stats.f_bfree, stats.f_blocks)) * block,
            availableBytes: available,
            purgeableBytes: important > 0 ? UInt64(important) - min(UInt64(important), available) : nil,
            snapshots: listSnapshots(mountPoint),
            containerVolumes: fileSystem == "apfs" ? containerVolumes(of: device) : []
        )
    }

    private static func text<T>(_ field: T) -> String {
        withUnsafeBytes(of: field) { buffer in
            String(decoding: buffer.prefix(while: { $0 != 0 }), as: UTF8.self)
        }
    }

    /// Local snapshot names from `tmutil listlocalsnapshots`. `fs_snapshot_list` needs root.
    private static func listSnapshots(_ mountPoint: String) -> [String]? {
        let process = Process()
        process.executableURL = URL(filePath: "/usr/bin/tmutil")
        process.arguments = ["listlocalsnapshots", mountPoint]
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return nil }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return parseSnapshots(String(decoding: data, as: UTF8.self))
    }

    /// The mounted volumes of a device's APFS container, with each one's used space.
    private static func containerVolumes(of device: String) -> [ContainerVolume] {
        guard let wanted = container(ofDevice: device) else { return [] }
        var table: UnsafeMutablePointer<statfs>?
        let count = getmntinfo(&table, MNT_NOWAIT)
        guard count > 0, let table else { return [] }
        var devices = Set<String>()
        var volumes: [ContainerVolume] = []
        for mount in UnsafeBufferPointer(start: table, count: Int(count)) {
            let mountDevice = text(mount.f_mntfromname)
            guard text(mount.f_fstypename) == "apfs", container(ofDevice: mountDevice) == wanted,
                  !devices.contains(mountDevice)
            else { continue }
            let path = text(mount.f_mntonname)
            if let used = spaceUsed(path) {
                devices.insert(mountDevice)
                volumes.append(ContainerVolume(path: path, usedBytes: used))
            }
        }
        return volumes.sorted { ($0.usedBytes, $1.path) > ($1.usedBytes, $0.path) }
    }

    /// A volume's own used space, `ATTR_VOL_SPACEUSED`. On APFS, `statfs` reports the
    /// whole container's.
    private static func spaceUsed(_ path: String) -> UInt64? {
        var request = attrlist(
            bitmapcount: u_short(ATTR_BIT_MAP_COUNT), reserved: 0, commonattr: 0,
            volattr: attrgroup_t(ATTR_VOL_INFO) | attrgroup_t(ATTR_VOL_SPACEUSED),
            dirattr: 0, fileattr: 0, forkattr: 0
        )
        var buffer = [UInt8](repeating: 0, count: 16)
        guard getattrlist(path, &request, &buffer, buffer.count, 0) == 0 else { return nil }
        let used = buffer.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 4, as: Int64.self) }
        return used >= 0 ? UInt64(used) : nil
    }
    #endif
}
