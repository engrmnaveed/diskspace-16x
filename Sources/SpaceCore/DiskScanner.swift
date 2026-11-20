import Dispatch
import Foundation

#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

public struct ScanFailure: LocalizedError, Sendable {
    public let path: String
    public let reason: String

    public var errorDescription: String? {
        "Unable to scan \(path.debugDescription). \(reason)"
    }
}

/// Reads directory entries and file metadata. Each scan uses its own traversal state.
public enum DiskScanner {
    public static func scan(
        path: String,
        progress: (@Sendable (ScanProgress) -> Void)? = nil,
        preview: (@Sendable (ScanReport) -> Void)? = nil,
        isCancelled: @escaping @Sendable () -> Bool = { false }
    ) throws -> ScanReport {
        guard !path.isEmpty, !path.utf8.contains(0) else {
            throw ScanFailure(path: path, reason: "Supply a valid directory path.")
        }
        // Preserve '..' so the OS resolves it correctly even after an intermediate symlink.
        // Remove trailing separators so O_NOFOLLOW applies to the selected directory itself.
        let absolute = path.utf8.first == 0x2F ? path : FileManager.default.currentDirectoryPath + "/" + path
        // Path separators are bytes, even when a filename begins with a combining mark.
        let components = absolute.utf8.split(separator: 0x2F)
            .map { String(decoding: $0, as: UTF8.self) }
            .filter { $0 != "." }
        let scope = "/" + components.joined(separator: "/")
        let rootName = components.last ?? "/"
        raiseOpenFileLimit()

        do {
            return try withDatalessMaterializationDisabled {
                let metadata = MetadataAccess(isCancelled: isCancelled)
                var updates: PreviewUpdates?
                if let preview {
                    var discovery = Traversal(depthLimit: 2, metadata: metadata, isCancelled: isCancelled)
                    let initial = try discovery.scan(path: scope, rootName: rootName)
                    guard initial.root.isPending else { return initial }
                    preview(ScanReport(root: initial.root, issues: initial.issues, isPreview: true))
                    updates = PreviewUpdates(root: initial.root, callback: preview)
                }
                var scan = Traversal(
                    progress: progress, previewUpdates: updates, metadata: metadata, isCancelled: isCancelled
                )
                return try scan.scan(path: scope, rootName: rootName)
            }
        } catch let error as DatalessPolicyError {
            throw ScanFailure(path: scope, reason: error.localizedDescription)
        }
    }
}

private struct Traversal {
    let progress: (@Sendable (ScanProgress) -> Void)?
    let isCancelled: @Sendable () -> Bool
    let depthLimit: Int?
    let metadata: MetadataAccess
    private var previewUpdates: PreviewUpdates?
    private var issues: [ScanIssue] = []
    private var hardLinks: Set<FileIdentity> = []
    private var visited: UInt64 = 0
    private var lastProgress: UInt64 = 0

    init(
        progress: (@Sendable (ScanProgress) -> Void)? = nil,
        depthLimit: Int? = nil,
        previewUpdates: PreviewUpdates? = nil,
        metadata: MetadataAccess,
        isCancelled: @escaping @Sendable () -> Bool
    ) {
        self.progress = progress
        self.depthLimit = depthLimit
        self.previewUpdates = previewUpdates
        self.metadata = metadata
        self.isCancelled = isCancelled
    }

    mutating func scan(path: String, rootName: String) throws -> ScanReport {
        try checkCancellation()
        let rootIsProvider = MetadataAccess.isProviderPath(path)
        let expected: stat
        do {
            expected = try readMetadata(path: path, name: path, parent: nil, isProvider: rootIsProvider)
        } catch let error as MetadataUnavailable {
            return unavailableRoot(path: path, name: rootName, error: error)
        } catch let problem as DirectoryFailure {
            throw ScanFailure(path: path, reason: problem.message)
        }
        switch expected.st_mode & mode_t(S_IFMT) {
        case mode_t(S_IFLNK):
            throw ScanFailure(path: path, reason: "Choose the directory the symbolic link points to.")
        case mode_t(S_IFDIR):
            break
        default:
            throw ScanFailure(path: path, reason: "Choose a directory.")
        }

        visited = 1
        reportProgress(at: path, force: true)
        if isCloudReference(expected) {
            return ScanReport(root: directoryEntry(path: path, name: rootName, cloudOnly: true), issues: [])
        }
        let rootListing: DirectoryListing
        do {
            rootListing = try readDirectory(path: path, name: path, parent: nil, expected: expected)
        } catch let error as MetadataUnavailable {
            return unavailableRoot(path: path, name: rootName, error: error)
        } catch let problem as DirectoryFailure {
            if problem.isCloudOnly {
                return ScanReport(root: directoryEntry(path: path, name: rootName, cloudOnly: true), issues: [])
            }
            throw ScanFailure(path: path, reason: problem.message)
        }
        if let error = rootListing.error {
            if error.isCloudOnly {
                return ScanReport(root: directoryEntry(path: path, name: rootName, cloudOnly: true), issues: [])
            }
            throw ScanFailure(path: path, reason: error.message)
        }

        let rootDevice = expected.st_dev
        #if os(macOS)
        let canonicalScope = path == "/" ? "/" : try? canonicalDirectoryPath(parent: rootListing.directory.descriptor, name: ".")
        let startupVolume = canonicalScope
            .flatMap { StartupVolume(scope: path, canonicalScope: $0) }
        #endif
        var ancestors = Set([FileIdentity(expected)])
        var stack = [DirectoryFrame(
            path: path,
            name: rootName,
            directory: rootListing.directory,
            names: rootListing.names
        )]

        // A frame keeps the directory open while its children are inspected relative to it.
        while let level = stack.indices.last {
            try checkCancellation()
            guard let name = stack[level].nextName() else {
                let finished = stack.removeLast()
                let entry = finished.entry
                if finished.didOverflow {
                    issues.append(ScanIssue(path: entry.path, message: "Directory totals exceed the supported byte range."))
                }
                guard let parent = stack.indices.last else {
                    reportProgress(at: path, force: true)
                    try checkCancellation()
                    return ScanReport(root: entry, issues: issues)
                }
                ancestors.remove(finished.directory.identity)
                add(entry, to: &stack[parent], parentDepth: parent)
                continue
            }

            let parentPath = stack[level].path
            let childPath = parentPath == "/" ? "/" + name.displayName : parentPath + "/" + name.displayName
            visited += 1
            reportProgress(at: childPath)
            guard name.isUTF8 else {
                issues.append(ScanIssue(path: childPath, message: "Skipped a filename that is not valid UTF-8. Its bytes are percent-encoded here."))
                continue
            }

            let parent = stack[level].directory
            let metadata: stat
            do {
                metadata = try readMetadata(
                    path: childPath, name: name.displayName, parent: parent, isProvider: parent.isProvider
                )
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                issues.append(ScanIssue(path: childPath, error: error))
                continue
            }
            guard metadata.st_dev == rootDevice else { continue }

            switch metadata.st_mode & mode_t(S_IFMT) {
            case mode_t(S_IFDIR):
                if isCloudReference(metadata) {
                    let entry = directoryEntry(path: childPath, name: name.displayName, cloudOnly: true)
                    add(entry, to: &stack[level], parentDepth: level)
                    continue
                }
                #if os(macOS)
                if let startupVolume, startupVolume.isBackingPath(childPath) {
                    do {
                        let canonical = try self.metadata.perform(at: childPath, isProvider: parent.isProvider) { _ in
                            try canonicalDirectoryPath(parent: parent.descriptor, name: name.displayName)
                        }
                        if startupVolume.isAlias(canonical) { continue }
                    } catch is CancellationError {
                        throw CancellationError()
                    } catch {
                        issues.append(ScanIssue(path: childPath, error: error))
                        continue
                    }
                }
                #endif
                if let depthLimit, level + 1 >= depthLimit {
                    stack[level].add(directoryEntry(path: childPath, name: name.displayName, pending: true))
                    continue
                }
                let identity = FileIdentity(metadata)
                guard !ancestors.contains(identity) else {
                    issues.append(ScanIssue(path: childPath, message: "Skipped a directory that points back to an ancestor."))
                    continue
                }
                do {
                    let listing = try readDirectory(
                        path: childPath, name: name.displayName, parent: parent, expected: metadata
                    )
                    if let error = listing.error {
                        if error.isCloudOnly { throw error }
                        issues.append(ScanIssue(path: childPath, error: error))
                    }
                    ancestors.insert(identity)
                    stack.append(DirectoryFrame(
                        path: childPath, name: name.displayName, directory: listing.directory, names: listing.names
                    ))
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    let cloudOnly = (error as? DirectoryFailure)?.isCloudOnly == true
                    if !cloudOnly {
                        issues.append(ScanIssue(path: childPath, error: error))
                    }
                    let entry = directoryEntry(path: childPath, name: name.displayName, cloudOnly: cloudOnly)
                    add(entry, to: &stack[level], parentDepth: level)
                }
            case mode_t(S_IFREG):
                let entry = file(name: name.displayName, path: childPath, metadata: metadata)
                add(entry, to: &stack[level], parentDepth: level)
            default:
                // Symlinks, sockets, pipes, and device nodes have no counted bytes.
                continue
            }
        }

        preconditionFailure("Traversal must finish at the root directory.")
    }

    private func unavailableRoot(path: String, name: String, error: MetadataUnavailable) -> ScanReport {
        ScanReport(
            root: directoryEntry(path: path, name: name),
            issues: [ScanIssue(path: path, message: error.localizedDescription)]
        )
    }

    private func readMetadata(path: String, name: String, parent: DirectoryHandle?, isProvider: Bool) throws -> stat {
        try metadata.perform(at: path, isProvider: isProvider) { _ in
            var value = stat()
            let result = name.withCString { rawPath in
                if let parent { return fstatat(parent.descriptor, rawPath, &value, AT_SYMLINK_NOFOLLOW) }
                return lstat(rawPath, &value)
            }
            guard result == 0 else { throw DirectoryFailure(operation: "Reading metadata", code: errno) }
            return value
        }
    }

    private func readDirectory(path: String, name: String, parent: DirectoryHandle?, expected: stat) throws -> DirectoryListing {
        let isProvider = parent?.isProvider == true || MetadataAccess.isProviderPath(path)
        return try metadata.perform(at: path, isProvider: isProvider) { check in
            let directory = try DirectoryHandle(
                parent: parent?.descriptor ?? AT_FDCWD, name: name, expected: expected, isProvider: isProvider
            )
            let listing = try directory.readNames(checkCancellation: check)
            return DirectoryListing(directory: directory, names: listing.names, error: listing.error)
        }
    }

    private mutating func file(name: String, path: String, metadata: stat) -> ScanEntry {
        let cloudOnly = isCloudReference(metadata)
        // An unseen deeper path can own the inode. Defer these bytes until the full
        // depth-first traversal assigns each hard link to its deterministic owner.
        let pending = depthLimit != nil && metadata.st_nlink > 1 && !cloudOnly
        let isAlias = !cloudOnly && !pending && metadata.st_nlink > 1
            && !hardLinks.insert(FileIdentity(metadata)).inserted
        let excluded = cloudOnly || pending || isAlias
        let logical = excluded ? 0 : UInt64(max(0, metadata.st_size))
        // st_blocks is expressed in 512-byte units on both supported platforms.
        let blocks = excluded ? 0 : UInt64(max(0, metadata.st_blocks))
        let (allocated, overflow) = blocks.multipliedReportingOverflow(by: 512)
        if overflow {
            issues.append(ScanIssue(path: path, message: "Allocated size exceeds the supported byte range."))
        }
        return ScanEntry(
            path: path, name: name, isDirectory: false, isHardLinkAlias: isAlias,
            isCloudOnly: cloudOnly, isPending: pending,
            logicalBytes: logical, allocatedBytes: overflow ? .max : allocated, children: []
        )
    }

    private mutating func add(_ entry: ScanEntry, to parent: inout DirectoryFrame, parentDepth: Int) {
        parent.add(entry)
        if parentDepth < 2, previewUpdates?.record(entry, in: parent.path) == true {
            previewUpdates?.publishIfReady(issues: issues)
        }
    }

    private func checkCancellation() throws {
        if isCancelled() || Task.isCancelled {
            throw CancellationError()
        }
    }

    private mutating func reportProgress(at path: String, force: Bool = false) {
        guard progress != nil || previewUpdates != nil else { return }
        let now = DispatchTime.now().uptimeNanoseconds
        guard force || now - lastProgress >= 100_000_000 else { return }
        lastProgress = now
        progress?(ScanProgress(visitedEntries: visited, currentPath: path))
        previewUpdates?.publishIfReady(issues: issues)
    }
}

#if os(macOS)
struct StartupVolume {
    private static let dataMount = "/System/Volumes/Data"
    private static let dataPrefix = dataMount + "/"
    private let canonicalScope: String
    private let backingPrefix: String

    init?(scope: String, canonicalScope: String) {
        let prefix = canonicalScope == "/" ? "/" : canonicalScope + "/"
        // A scan started at or inside the Data mount treats it as its own scope.
        guard Self.dataMount.utf8.starts(with: prefix.utf8) else { return nil }
        let relativeMount = Self.dataPrefix.dropFirst(prefix.count)
        self.canonicalScope = prefix
        self.backingPrefix = (scope == "/" ? "/" : scope + "/") + relativeMount
    }

    func isBackingPath(_ path: String) -> Bool {
        path.utf8.starts(with: backingPrefix.utf8)
    }

    func isAlias(_ canonical: String) -> Bool {
        // Keep firmlinks in their visible locations and retain Data-only directories.
        canonical != "/"
            && canonical.utf8.starts(with: canonicalScope.utf8)
            && !canonical.utf8.starts(with: Self.dataPrefix.utf8)
            && canonical != Self.dataMount
    }
}

func canonicalDirectoryPath(parent: Int32, name: String) throws -> String {
    let operation = "Checking for an APFS firmlink"
    var attributes = attrlist()
    attributes.bitmapcount = UInt16(ATTR_BIT_MAP_COUNT)
    attributes.commonattr = attrgroup_t(ATTR_CMN_FULLPATH)
    // ATTR_CMN_FULLPATH reports the visible path even when reached through the Data mount.
    // The only requested attribute is a path reference immediately after the buffer length.
    let referenceOffset = MemoryLayout<UInt32>.size
    let pathOffset = referenceOffset + MemoryLayout<attrreference_t>.size
    var buffer = [UInt8](repeating: 0, count: pathOffset + Int(MAXPATHLEN))
    return try buffer.withUnsafeMutableBytes { bytes in
        let result = name.withCString {
            getattrlistat(parent, $0, &attributes, bytes.baseAddress, bytes.count, UInt(FSOPT_NOFOLLOW))
        }
        guard result == 0 else { throw DirectoryFailure(operation: operation, code: errno) }
        let reference = bytes.loadUnaligned(fromByteOffset: referenceOffset, as: attrreference_t.self)
        let length = Int(reference.attr_length)
        guard reference.attr_dataoffset == pathOffset - referenceOffset,
              length > 1, length <= Int(MAXPATHLEN), bytes[pathOffset + length - 1] == 0,
              let path = String(bytes: bytes[pathOffset..<(pathOffset + length - 1)], encoding: .utf8),
              path.utf8.first == 0x2F else {
            throw DirectoryFailure(operation: operation, code: EIO)
        }
        return path
    }
}
#endif

private struct FileIdentity: Equatable, Hashable {
    let device: dev_t
    let inode: ino_t

    init(_ metadata: stat) {
        self.device = metadata.st_dev
        self.inode = metadata.st_ino
    }
}

private struct DirectoryListing: Sendable {
    let directory: DirectoryHandle
    let names: [DirectoryName]
    let error: DirectoryFailure?
}

private struct DirectoryName: Sendable {
    let displayName: String
    let isUTF8: Bool
}

private struct DirectoryFailure: LocalizedError {
    let operation: String
    let code: Int32

    var isCloudOnly: Bool {
        #if canImport(Darwin)
        code == EDEADLK
        #else
        false
        #endif
    }

    var message: String { directoryError(operation, code) }
    var errorDescription: String? { message }
}

extension ScanIssue {
    /// Records a failure, separating access refusals from other errors.
    fileprivate init(path: String, error: Error) {
        let kind: Kind
        switch (error as? DirectoryFailure)?.code {
        case EACCES?: kind = .permissionDenied
        case EPERM?: kind = .notPermitted
        default: kind = .error
        }
        self.init(path: path, message: error.localizedDescription, kind: kind)
    }
}

// The stream is enumerated on its opening thread before publishing the handle.
// Subsequent metadata calls only use its immutable descriptor and retain it.
private final class DirectoryHandle: @unchecked Sendable {
    #if canImport(Darwin)
    private let stream: UnsafeMutablePointer<DIR>
    #else
    private let stream: OpaquePointer
    #endif
    let descriptor: Int32
    let identity: FileIdentity
    let isProvider: Bool

    init(parent: Int32, name: String, expected: stat, isProvider: Bool) throws {
        let descriptor = name.withCString { openat(parent, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC) }
        guard descriptor >= 0 else {
            throw DirectoryFailure(operation: "Opening directory", code: errno)
        }
        var adopted = false
        defer {
            if !adopted { _ = close(descriptor) }
        }
        var actual = stat()
        guard fstat(descriptor, &actual) == 0 else {
            throw DirectoryFailure(operation: "Reading directory metadata", code: errno)
        }
        let identity = FileIdentity(actual)
        guard identity == FileIdentity(expected) else {
            throw DirectoryFailure(operation: "Reopening directory after its identity changed", code: ESTALE)
        }
        guard let stream = fdopendir(descriptor) else {
            throw DirectoryFailure(operation: "Opening directory stream", code: errno)
        }
        adopted = true
        self.descriptor = descriptor
        self.identity = identity
        self.isProvider = isProvider
        self.stream = stream
    }

    deinit {
        _ = closedir(stream)
    }

    func readNames(checkCancellation: () throws -> Void) throws -> (names: [DirectoryName], error: DirectoryFailure?) {
        var names: [DirectoryName] = []
        var error: DirectoryFailure?
        while true {
            try checkCancellation()
            errno = 0
            guard let entry = readdir(stream) else {
                let code = errno
                if code != 0 { error = DirectoryFailure(operation: "Listing directory", code: code) }
                break
            }
            let name = withUnsafeBytes(of: &entry.pointee.d_name) { buffer in
                let bytes = buffer.prefix { $0 != 0 }
                if let string = String(bytes: bytes, encoding: .utf8) {
                    return DirectoryName(displayName: string, isUTF8: true)
                }
                let escaped = bytes.map { String(format: "%%%02X", Int($0)) }.joined()
                return DirectoryName(displayName: escaped, isUTF8: false)
            }
            if name.displayName != "." && name.displayName != ".." {
                names.append(name)
            }
        }
        names.sort { $0.displayName.utf8.lexicographicallyPrecedes($1.displayName.utf8) }
        return (names, error)
    }
}

/// Publishes completed branches against the shallow tree. Snapshots reuse finished
/// subtrees and never copy or walk the full tree to report progress.
private struct PreviewUpdates {
    private var root: ScanEntry
    private let callback: @Sendable (ScanReport) -> Void
    private var unresolvedPaths: Set<String> = []
    private var replacements: [String: [String: ScanEntry]] = [:]
    private var lastPublication = DispatchTime.now().uptimeNanoseconds

    init(root: ScanEntry, callback: @escaping @Sendable (ScanReport) -> Void) {
        self.root = root
        self.callback = callback
        for child in root.children where child.isPending {
            unresolvedPaths.insert(child.path)
            for grandchild in child.children where grandchild.isPending {
                unresolvedPaths.insert(grandchild.path)
            }
        }
    }

    mutating func record(_ entry: ScanEntry, in parent: String) -> Bool {
        guard unresolvedPaths.remove(entry.path) != nil else { return false }
        replacements[parent, default: [:]][entry.path] = entry
        return true
    }

    mutating func publishIfReady(issues: [ScanIssue]) {
        guard !replacements.isEmpty else { return }
        let now = DispatchTime.now().uptimeNanoseconds
        guard now - lastPublication >= 400_000_000 else { return }
        let direct = replacements[root.path] ?? [:]
        let children = root.children.map { child in
            if let finished = direct[child.path] { return finished }
            guard let nested = replacements[child.path] else { return child }
            return child.withChildren(child.children.map { nested[$0.path] ?? $0 })
        }
        root = root.withChildren(children)
        replacements.removeAll(keepingCapacity: true)
        lastPublication = now
        callback(ScanReport(root: root, issues: issues, isPreview: true))
    }
}

private extension ScanEntry {
    func withChildren(_ children: [ScanEntry]) -> ScanEntry {
        var logical: UInt64 = 0
        var allocated: UInt64 = 0
        var pending = false
        for entry in children {
            let (nextLogical, logicalOverflow) = logical.addingReportingOverflow(entry.logicalBytes)
            let (nextAllocated, allocatedOverflow) = allocated.addingReportingOverflow(entry.allocatedBytes)
            logical = logicalOverflow ? .max : nextLogical
            allocated = allocatedOverflow ? .max : nextAllocated
            pending = pending || entry.isPending
        }
        return ScanEntry(
            path: path, name: name, isDirectory: true, isHardLinkAlias: false,
            isCloudOnly: false, isPending: pending, logicalBytes: logical,
            allocatedBytes: allocated, children: children
        )
    }
}

private struct DirectoryFrame {
    let path: String
    let name: String
    let directory: DirectoryHandle
    let names: [DirectoryName]
    private var nextIndex = 0
    private var logical: UInt64 = 0
    private var allocated: UInt64 = 0
    private var children: [ScanEntry] = []
    private(set) var didOverflow = false
    private var isPending = false

    var entry: ScanEntry {
        ScanEntry(
            path: path, name: name, isDirectory: true, isHardLinkAlias: false,
            isCloudOnly: false, isPending: isPending,
            logicalBytes: logical, allocatedBytes: allocated, children: children
        )
    }

    init(path: String, name: String, directory: DirectoryHandle, names: [DirectoryName]) {
        self.path = path
        self.name = name
        self.directory = directory
        self.names = names
    }

    mutating func nextName() -> DirectoryName? {
        guard nextIndex < names.count else { return nil }
        defer { nextIndex += 1 }
        return names[nextIndex]
    }

    mutating func add(_ entry: ScanEntry) {
        isPending = isPending || entry.isPending
        let (newLogical, logicalOverflow) = logical.addingReportingOverflow(entry.logicalBytes)
        let (newAllocated, allocatedOverflow) = allocated.addingReportingOverflow(entry.allocatedBytes)
        logical = logicalOverflow ? .max : newLogical
        allocated = allocatedOverflow ? .max : newAllocated
        didOverflow = didOverflow || logicalOverflow || allocatedOverflow
        children.append(entry)
    }
}

private func directoryEntry(path: String, name: String, cloudOnly: Bool = false, pending: Bool = false) -> ScanEntry {
    ScanEntry(
        path: path, name: name, isDirectory: true, isHardLinkAlias: false,
        isCloudOnly: cloudOnly, isPending: pending,
        logicalBytes: 0, allocatedBytes: 0, children: []
    )
}

/// The traversal keeps one directory descriptor open per level. launchd starts GUI apps
/// with a soft limit of 256, so deep trees would otherwise fail with EMFILE. Raise the
/// soft limit toward OPEN_MAX without exceeding the hard limit. This never lowers it.
private func raiseOpenFileLimit() {
    #if canImport(Darwin)
    var limit = rlimit()
    guard getrlimit(RLIMIT_NOFILE, &limit) == 0 else { return }
    let target = min(rlim_t(OPEN_MAX), limit.rlim_max)
    guard limit.rlim_cur < target else { return }
    limit.rlim_cur = target
    _ = setrlimit(RLIMIT_NOFILE, &limit)
    #endif
}

private func isCloudReference(_ metadata: stat) -> Bool {
    #if canImport(Darwin)
    metadata.st_flags & UInt32(SF_DATALESS) != 0
    #else
    false
    #endif
}

private func directoryError(_ operation: String, _ code: Int32) -> String {
    #if canImport(Darwin)
    if code == EDEADLK { return "This path needs cloud content that is unavailable locally." }
    #endif
    return posixMessage(operation, code)
}

private func posixMessage(_ operation: String, _ code: Int32) -> String {
    "\(operation) failed (\(String(cString: strerror(code))))."
}
