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
///
/// A scan reads several directories at once in a task group, one directory per child
/// task. The scan's own task decides which directory comes next, keeps the results,
/// reports progress, and publishes previews. Results are assembled depth first, which is
/// also where each hard-linked file is assigned to its first path, so the report doesn't
/// depend on which directory finished first.
public enum DiskScanner {
    /// Directories read at once: one per core, up to 8.
    static let defaultConcurrency = max(1, min(ProcessInfo.processInfo.activeProcessorCount, 8))

    public static func scan(
        path: String,
        excludes: ExcludeRules = ExcludeRules(),
        progress: (@Sendable (ScanProgress) -> Void)? = nil,
        preview: (@Sendable (ScanReport) -> Void)? = nil,
        isCancelled: @escaping @Sendable () -> Bool = { false }
    ) async throws -> ScanReport {
        try await scan(
            path: path, excludes: excludes, concurrency: defaultConcurrency,
            progress: progress, preview: preview, isCancelled: isCancelled
        )
    }

    /// `onRead` runs in each directory's task before it reads, for tests.
    static func scan(
        path: String,
        excludes: ExcludeRules = ExcludeRules(),
        concurrency: Int,
        progress: (@Sendable (ScanProgress) -> Void)? = nil,
        preview: (@Sendable (ScanReport) -> Void)? = nil,
        isCancelled: @escaping @Sendable () -> Bool = { false },
        onRead: (@Sendable () -> Void)? = nil
    ) async throws -> ScanReport {
        let (scope, rootName) = try normalized(path)
        raiseOpenFileLimit()
        let start = try Walk.start(
            scope: scope, rootName: rootName, excludes: excludes, excludeRoot: nil,
            isCancelled: isCancelled, onRead: onRead
        )
        switch start {
        case .finished(let report):
            return report
        case .walk(let walk, let root):
            if preview == nil { progress?(ScanProgress(visitedEntries: 1, currentPath: scope)) }
            let coordinator = Coordinator(
                walk: walk, concurrency: concurrency, progress: progress, preview: preview
            )
            return try await coordinator.run(root: root)
        }
    }

    /// Scans one folder of an earlier scan of `root` again, without previews. Exclude
    /// patterns stay relative to `root`. Merge the result with `ScanReport.replacing(_:)`.
    ///
    /// Hard links are counted once within the folder. A file that also has a link outside
    /// it may be counted on both sides until a full rescan.
    public static func rescan(
        folder: String,
        in root: String,
        excludes: ExcludeRules = ExcludeRules(),
        progress: (@Sendable (ScanProgress) -> Void)? = nil,
        isCancelled: @escaping @Sendable () -> Bool = { false }
    ) async throws -> ScanReport {
        let (scope, name) = try normalized(folder)
        let (base, _) = try normalized(root)
        guard scope == base || scope.hasPrefix(base == "/" ? "/" : base + "/") else {
            throw ScanFailure(path: scope, reason: "The folder isn't inside the scanned folder.")
        }
        raiseOpenFileLimit()
        let start = try Walk.start(
            scope: scope, rootName: name, excludes: excludes, excludeRoot: base,
            isCancelled: isCancelled, onRead: nil
        )
        switch start {
        case .finished(let report):
            return report
        case .walk(let walk, let job):
            progress?(ScanProgress(visitedEntries: 1, currentPath: scope))
            let coordinator = Coordinator(
                walk: walk, concurrency: defaultConcurrency, progress: progress, preview: nil
            )
            return try await coordinator.run(root: job)
        }
    }

    /// An absolute scope path and its display name.
    private static func normalized(_ path: String) throws -> (scope: String, name: String) {
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
        return ("/" + components.joined(separator: "/"), components.last ?? "/")
    }
}

/// One directory waiting to be read.
private struct Job: Sendable {
    let path: String
    let name: String
    let identity: FileIdentity
    /// The open parent directory. Children are opened relative to it, so a renamed or
    /// replaced ancestor can't redirect the scan. The root arrives already listed.
    let parent: DirectoryHandle?
    let listing: DirectoryListing?
    let depth: Int
    /// Identities of the directories above this one, to skip loops.
    let ancestors: [FileIdentity]
    /// The folder holding this one.
    let container: ScanLocation?
}

/// What one directory's task read.
private struct DirectoryResult: Sendable {
    let name: String
    let container: ScanLocation?
    /// The location this directory's children share.
    let location: ScanLocation
    var isCloudOnly = false
    var children: [RawChild] = []
    var subjobs: [Job] = []
    var issues: [ScanIssue] = []
    var excluded: UInt64 = 0
    var visited: UInt64 = 0
    var hasLinks = false
}

private enum RawChild: Sendable {
    case entry(ScanEntry)
    /// A file with more than one hard link, counted only at its first path.
    case linked(ScanEntry, FileIdentity)
    /// A subdirectory, by its index among the result's new jobs.
    case folder(Int, name: String)
}

/// The rules every directory task follows, shared read-only.
private struct Walk: Sendable {
    enum Start {
        case finished(ScanReport)
        case walk(Walk, Job)
    }

    let excludes: ExcludeRules
    /// The path exclude patterns are relative to. A folder rescan keeps the original
    /// scan root, so anchored patterns such as `/build` still apply there.
    let excludeBase: String
    let rootDevice: dev_t
    #if os(macOS)
    let startupVolume: StartupVolume?
    #endif
    let metadata: MetadataAccess
    let isCancelled: @Sendable () -> Bool
    let onRead: (@Sendable () -> Void)?

    /// Reads the root, which must be a readable directory, or returns the whole report
    /// when the root is cloud-only or its provider doesn't respond.
    static func start(
        scope path: String, rootName: String, excludes: ExcludeRules, excludeRoot: String?,
        isCancelled: @escaping @Sendable () -> Bool, onRead: (@Sendable () -> Void)?
    ) throws -> Start {
        do {
            return try withDatalessMaterializationDisabled {
                try startUnchecked(
                    scope: path, rootName: rootName, excludes: excludes, excludeRoot: excludeRoot,
                    isCancelled: isCancelled, onRead: onRead
                )
            }
        } catch let error as DatalessPolicyError {
            throw ScanFailure(path: path, reason: error.localizedDescription)
        }
    }

    private static func startUnchecked(
        scope path: String, rootName: String, excludes: ExcludeRules, excludeRoot: String?,
        isCancelled: @escaping @Sendable () -> Bool, onRead: (@Sendable () -> Void)?
    ) throws -> Start {
        if isCancelled() || Task.isCancelled { throw CancellationError() }
        let metadata = MetadataAccess(isCancelled: isCancelled)
        // The folder holding the scope. A scan of "/" has none, and its root's name is "/".
        let rootContainer = path == "/" ? nil : ScanLocation(path: parentPath(of: path))
        let rootIsProvider = MetadataAccess.isProviderPath(path)
        func unavailable(_ error: MetadataUnavailable) -> Start {
            .finished(ScanReport(
                root: directoryEntry(name: rootName, container: rootContainer),
                issues: [ScanIssue(path: path, message: error.localizedDescription)]
            ))
        }
        let cloudRoot = Start.finished(ScanReport(
            root: directoryEntry(name: rootName, container: rootContainer, cloudOnly: true), issues: []
        ))
        let expected: stat
        do {
            expected = try readMetadata(
                metadata, path: path, name: path, parent: nil, isProvider: rootIsProvider
            )
        } catch let error as MetadataUnavailable {
            return unavailable(error)
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
        if isCloudReference(expected) { return cloudRoot }
        let identity = FileIdentity(expected)
        let listing: DirectoryListing
        do {
            listing = try readDirectory(metadata, path: path, name: path, parent: nil, expected: identity)
        } catch let error as MetadataUnavailable {
            return unavailable(error)
        } catch let problem as DirectoryFailure {
            if problem.isCloudOnly { return cloudRoot }
            throw ScanFailure(path: path, reason: problem.message)
        }
        if let error = listing.error {
            if error.isCloudOnly { return cloudRoot }
            throw ScanFailure(path: path, reason: error.message)
        }
        #if os(macOS)
        let canonicalScope = path == "/" ? "/" : try? canonicalDirectoryPath(parent: listing.directory.descriptor, name: ".")
        let startupVolume = canonicalScope.flatMap { StartupVolume(scope: path, canonicalScope: $0) }
        #endif
        #if os(macOS)
        let walk = Walk(
            excludes: excludes, excludeBase: excludeRoot ?? path, rootDevice: expected.st_dev,
            startupVolume: startupVolume,
            metadata: metadata, isCancelled: isCancelled, onRead: onRead
        )
        #else
        let walk = Walk(
            excludes: excludes, excludeBase: excludeRoot ?? path, rootDevice: expected.st_dev,
            metadata: metadata, isCancelled: isCancelled, onRead: onRead
        )
        #endif
        let root = Job(
            path: path, name: rootName, identity: identity, parent: nil, listing: listing,
            depth: 0, ancestors: [], container: rootContainer
        )
        return .walk(walk, root)
    }

    /// Lists one directory and reads its children's metadata, with cloud downloads
    /// turned off for this thread. A task's body doesn't suspend, so it stays on one
    /// thread throughout.
    func read(_ job: Job) throws -> DirectoryResult {
        do {
            return try withDatalessMaterializationDisabled { try readUnchecked(job) }
        } catch let error as DatalessPolicyError {
            throw ScanFailure(path: job.path, reason: error.localizedDescription)
        }
    }

    private func readUnchecked(_ job: Job) throws -> DirectoryResult {
        onRead?()
        try checkCancellation()
        var result = DirectoryResult(
            name: job.name, container: job.container, location: ScanLocation(path: job.path)
        )
        let listing: DirectoryListing
        if let prepared = job.listing {
            listing = prepared
        } else {
            do {
                listing = try Self.readDirectory(
                    metadata, path: job.path, name: job.name, parent: job.parent, expected: job.identity
                )
                if let error = listing.error {
                    if error.isCloudOnly { throw error }
                    result.issues.append(ScanIssue(path: job.path, error: error))
                }
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                let cloudOnly = (error as? DirectoryFailure)?.isCloudOnly == true
                if !cloudOnly {
                    result.issues.append(ScanIssue(path: job.path, error: error))
                }
                result.isCloudOnly = cloudOnly
                return result
            }
        }

        let parent = listing.directory
        let chain = job.ancestors + [job.identity]
        for name in listing.names {
            try checkCancellation()
            let childPath = job.path == "/" ? "/" + name.displayName : job.path + "/" + name.displayName
            let relative: [UInt8]? = excludes.isEmpty || !name.isUTF8
                ? nil
                : Array(childPath.utf8.dropFirst(excludeBase == "/" ? 1 : excludeBase.utf8.count + 1))
            if let relative, excludes.matchesAnyType(relative: relative, name: Array(name.displayName.utf8)) {
                result.excluded += 1
                continue
            }
            result.visited += 1
            guard name.isUTF8 else {
                result.issues.append(ScanIssue(path: childPath, message: "Skipped a filename that is not valid UTF-8. Its bytes are percent-encoded here."))
                continue
            }

            let metadata: stat
            do {
                metadata = try Self.readMetadata(
                    self.metadata, path: childPath, name: name.displayName, parent: parent,
                    isProvider: parent.isProvider
                )
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                result.issues.append(ScanIssue(path: childPath, error: error))
                continue
            }
            guard metadata.st_dev == rootDevice else { continue }

            switch metadata.st_mode & mode_t(S_IFMT) {
            case mode_t(S_IFDIR):
                if let relative, excludes.matchesDirectory(relative: relative, name: Array(name.displayName.utf8)) {
                    result.excluded += 1
                    continue
                }
                if isCloudReference(metadata) {
                    let entry = directoryEntry(name: name.displayName, container: result.location, cloudOnly: true)
                    result.children.append(.entry(entry))
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
                        result.issues.append(ScanIssue(path: childPath, error: error))
                        continue
                    }
                }
                #endif
                let identity = FileIdentity(metadata)
                guard !chain.contains(identity) else {
                    result.issues.append(ScanIssue(path: childPath, message: "Skipped a directory that points back to an ancestor."))
                    continue
                }
                result.children.append(.folder(result.subjobs.count, name: name.displayName))
                result.subjobs.append(Job(
                    path: childPath, name: name.displayName, identity: identity, parent: parent,
                    listing: nil, depth: job.depth + 1, ancestors: chain, container: result.location
                ))
            case mode_t(S_IFREG):
                let child = file(name: name.displayName, in: result.location, path: childPath, metadata: metadata, issues: &result.issues)
                if case .linked = child { result.hasLinks = true }
                result.children.append(child)
            default:
                // Symlinks, sockets, pipes, and device nodes have no counted bytes.
                continue
            }
        }
        return result
    }

    private func file(
        name: String, in folder: ScanLocation, path: String, metadata: stat, issues: inout [ScanIssue]
    ) -> RawChild {
        let cloudOnly = isCloudReference(metadata)
        let logical = cloudOnly ? 0 : UInt64(max(0, metadata.st_size))
        // st_blocks is expressed in 512-byte units on both supported platforms.
        let blocks = cloudOnly ? 0 : UInt64(max(0, metadata.st_blocks))
        let (allocated, overflow) = blocks.multipliedReportingOverflow(by: 512)
        if overflow {
            issues.append(ScanIssue(path: path, message: "Allocated size exceeds the supported byte range."))
        }
        let entry = ScanEntry(
            name: name, container: folder, isDirectory: false, isHardLinkAlias: false,
            isCloudOnly: cloudOnly, isPending: false,
            logicalBytes: logical, allocatedBytes: overflow ? .max : allocated, itemCount: 0, children: []
        )
        // Another path may own this inode. The depth-first assembly picks the first one.
        if !cloudOnly && metadata.st_nlink > 1 { return .linked(entry, FileIdentity(metadata)) }
        return .entry(entry)
    }

    private static func readMetadata(
        _ access: MetadataAccess, path: String, name: String, parent: DirectoryHandle?, isProvider: Bool
    ) throws -> stat {
        try access.perform(at: path, isProvider: isProvider) { _ in
            var value = stat()
            let result = name.withCString { rawPath in
                if let parent { return fstatat(parent.descriptor, rawPath, &value, AT_SYMLINK_NOFOLLOW) }
                return lstat(rawPath, &value)
            }
            guard result == 0 else { throw DirectoryFailure(operation: "Reading metadata", code: errno) }
            return value
        }
    }

    private static func readDirectory(
        _ access: MetadataAccess, path: String, name: String, parent: DirectoryHandle?, expected: FileIdentity
    ) throws -> DirectoryListing {
        let isProvider = parent?.isProvider == true || MetadataAccess.isProviderPath(path)
        return try access.perform(at: path, isProvider: isProvider) { check in
            let directory = try DirectoryHandle(
                parent: parent?.descriptor ?? AT_FDCWD, name: name, expected: expected, isProvider: isProvider
            )
            let listing = try directory.readNames(checkCancellation: check)
            return DirectoryListing(directory: directory, names: listing.names, error: listing.error)
        }
    }

    func checkCancellation() throws {
        if isCancelled() || Task.isCancelled {
            throw CancellationError()
        }
    }
}

/// A directory's read result, kept until its folder is assembled.
private struct Record {
    let name: String
    let container: ScanLocation?
    let location: ScanLocation
    let isCloudOnly: Bool
    let children: [RawChild]
    /// The index of the first job this directory added.
    let firstJob: Int
}

/// The scan's own task: schedules directory tasks, keeps their results, reports
/// progress and previews, and assembles the report. It reads the root and its folders
/// first, so a preview can show two levels, then everything below, newest first, which
/// keeps the walk close to depth first and the number of open directories small.
private final class Coordinator {
    private let walk: Walk
    private let concurrency: Int
    private let progress: (@Sendable (ScanProgress) -> Void)?
    private let preview: (@Sendable (ScanReport) -> Void)?

    private var records: [Record?] = [nil]
    private var depths: [Int] = [0]
    /// The second-level folder each job belongs to.
    private var segments: [Int?] = [nil]
    /// Second-level folders with jobs still running or waiting.
    private var outstanding: [Int: Int] = [:]
    /// Finished second-level folders without hard links, assembled once.
    private var finished: [Int: ScanEntry] = [:]
    private var linkedSegments: Set<Int> = []
    private var issues: [ScanIssue] = []
    private var excluded: UInt64 = 0
    private var visited: UInt64 = 1
    private var currentPath: String
    private var lastProgress: UInt64 = 0
    /// With previews, progress starts once the two-level preview is out.
    private var reportsProgress: Bool

    init(
        walk: Walk, concurrency: Int,
        progress: (@Sendable (ScanProgress) -> Void)?, preview: (@Sendable (ScanReport) -> Void)?
    ) {
        self.walk = walk
        self.concurrency = max(1, concurrency)
        self.progress = progress
        self.preview = preview
        self.currentPath = ""
        self.reportsProgress = preview == nil
    }

    func run(root: Job) async throws -> ScanReport {
        let scope = root.path
        currentPath = scope
        var shallow: [(Int, Job)] = [(0, root)]
        var nextShallow = 0
        var deep: [(Int, Job)] = []
        var open = preview == nil
        var published: UInt64?
        var changed = false
        let walk = self.walk
        try await withThrowingTaskGroup(of: (Int, DirectoryResult).self) { group in
            var running = 0
            while true {
                while running < concurrency {
                    let next: (Int, Job)
                    if nextShallow < shallow.count {
                        next = shallow[nextShallow]
                        nextShallow += 1
                    } else if open, let job = deep.popLast() {
                        next = job
                    } else {
                        break
                    }
                    let (index, job) = next
                    group.addTask { (index, try walk.read(job)) }
                    running += 1
                }
                if running == 0 {
                    guard !open else { break }
                    // The root and its folders are read. Show them before going deeper.
                    // Jobs queued so far are in name order, so the first comes out first.
                    open = true
                    deep.reverse()
                    if let preview, !deep.isEmpty {
                        published = DispatchTime.now().uptimeNanoseconds
                        preview(snapshot())
                        try walk.checkCancellation()
                        reportsProgress = true
                        report(force: true)
                    }
                    continue
                }
                guard let (index, result) = try await group.next() else { break }
                running -= 1
                let (added, segmentDone) = store(result, at: index)
                for (jobIndex, job) in added where job.depth < 2 {
                    shallow.append((jobIndex, job))
                }
                let deeper = added.filter { $0.1.depth >= 2 }
                // Once open, the stack takes each directory's first child first.
                deep.append(contentsOf: open ? deeper.reversed() : deeper)
                changed = changed || segmentDone
                report(force: false)
                if let preview, changed, let last = published, running + deep.count > 0,
                   DispatchTime.now().uptimeNanoseconds - last >= 400_000_000 {
                    changed = false
                    published = DispatchTime.now().uptimeNanoseconds
                    preview(snapshot())
                }
            }
        }
        try walk.checkCancellation()
        let (rootEntry, overflowed) = compose(from: 0, preview: false, consume: true)
        issues += overflowed
        currentPath = scope
        report(force: true)
        return ScanReport(root: rootEntry, issues: issues.sorted(by: pathOrder), excludedCount: excluded)
    }

    /// Keeps a result and numbers the jobs it added. Returns them and whether this
    /// finished a second-level folder.
    private func store(_ result: DirectoryResult, at index: Int) -> ([(Int, Job)], Bool) {
        let firstJob = records.count
        records[index] = Record(
            name: result.name, container: result.container, location: result.location,
            isCloudOnly: result.isCloudOnly, children: result.children, firstJob: firstJob
        )
        var added: [(Int, Job)] = []
        added.reserveCapacity(result.subjobs.count)
        for job in result.subjobs {
            let jobIndex = records.count
            records.append(nil)
            depths.append(job.depth)
            switch job.depth {
            case ..<2:
                segments.append(nil)
            case 2:
                segments.append(jobIndex)
                outstanding[jobIndex] = 1
            default:
                let segment = segments[index]!
                segments.append(segment)
                outstanding[segment, default: 0] += 1
            }
            added.append((jobIndex, job))
        }
        issues += result.issues
        excluded += result.excluded
        visited += result.visited
        currentPath = result.location.path

        guard let segment = segments[index] else { return (added, false) }
        if result.hasLinks { linkedSegments.insert(segment) }
        let remaining = outstanding[segment, default: 1] - 1
        guard remaining == 0 else {
            outstanding[segment] = remaining
            return (added, false)
        }
        outstanding[segment] = nil
        // Without hard links, the folder doesn't depend on anything else in the scan.
        if !linkedSegments.contains(segment) {
            let (entry, overflowed) = compose(from: segment, preview: false, consume: true)
            finished[segment] = entry
            issues += overflowed
        }
        return (added, true)
    }

    private func report(force: Bool) {
        guard reportsProgress, let progress else { return }
        let now = DispatchTime.now().uptimeNanoseconds
        guard force || now - lastProgress >= 100_000_000 else { return }
        lastProgress = now
        progress(ScanProgress(visitedEntries: visited, currentPath: currentPath))
    }

    private func snapshot() -> ScanReport {
        let (root, _) = compose(from: 0, preview: true, consume: false)
        return ScanReport(
            root: root, issues: issues.sorted(by: pathOrder), isPreview: true, excludedCount: excluded
        )
    }

    /// Builds the folder for record `start` and everything below it, depth first. A
    /// preview shows second-level folders that aren't finished as pending, and so are
    /// hard-linked files that such a folder might own. Returns the folder and issues for
    /// folders whose totals exceed the byte range.
    private func compose(from start: Int, preview: Bool, consume: Bool) -> (ScanEntry, [ScanIssue]) {
        var seen = Set<FileIdentity>()
        var pendingBefore = false
        var overflowed: [ScanIssue] = []
        var stack = [Frame(index: start)]
        while let top = stack.last {
            guard let record = records[top.index] else {
                preconditionFailure("A folder is assembled only after its directory is read.")
            }
            guard top.next < record.children.count else {
                let frame = stack.removeLast()
                let entry = ScanEntry(
                    name: record.name, container: record.container, isDirectory: true,
                    isHardLinkAlias: false, isCloudOnly: record.isCloudOnly, isPending: frame.isPending,
                    logicalBytes: frame.logical, allocatedBytes: frame.allocated,
                    itemCount: frame.items, children: frame.children
                )
                if frame.didOverflow {
                    overflowed.append(ScanIssue(path: entry.path, message: "Directory totals exceed the supported byte range."))
                }
                if consume { records[frame.index] = nil }
                guard !stack.isEmpty else { return (entry, overflowed) }
                stack[stack.count - 1].add(entry)
                continue
            }
            let child = record.children[top.next]
            stack[stack.count - 1].next += 1
            switch child {
            case .entry(let entry):
                stack[stack.count - 1].add(entry)
            case .linked(let entry, let identity):
                if !seen.insert(identity).inserted {
                    stack[stack.count - 1].add(entry.uncounted(alias: true))
                } else if pendingBefore {
                    stack[stack.count - 1].add(entry.uncounted(alias: false))
                } else {
                    stack[stack.count - 1].add(entry)
                }
            case .folder(let offset, let name):
                let job = record.firstJob + offset
                if let entry = finished[job] {
                    stack[stack.count - 1].add(entry)
                } else if preview && (depths[job] == 2 && outstanding[job] != nil || records[job] == nil) {
                    pendingBefore = true
                    stack[stack.count - 1].add(directoryEntry(name: name, container: record.location, pending: true))
                } else {
                    stack.append(Frame(index: job))
                }
            }
        }
        preconditionFailure("Assembly finishes at the starting folder.")
    }
}

/// A folder being assembled.
private struct Frame {
    let index: Int
    var next = 0
    var children: [ScanEntry] = []
    var logical: UInt64 = 0
    var allocated: UInt64 = 0
    var items: UInt64 = 0
    var isPending = false
    var didOverflow = false

    init(index: Int) {
        self.index = index
    }

    mutating func add(_ entry: ScanEntry) {
        isPending = isPending || entry.isPending
        let (newLogical, logicalOverflow) = logical.addingReportingOverflow(entry.logicalBytes)
        let (newAllocated, allocatedOverflow) = allocated.addingReportingOverflow(entry.allocatedBytes)
        logical = logicalOverflow ? .max : newLogical
        allocated = allocatedOverflow ? .max : newAllocated
        didOverflow = didOverflow || logicalOverflow || allocatedOverflow
        items = items.addingCounted(entry)
        children.append(entry)
    }
}

private extension ScanEntry {
    /// This file without its bytes: an alias of a file counted at another path, or a
    /// link whose first path isn't known yet.
    func uncounted(alias: Bool) -> ScanEntry {
        ScanEntry(
            name: name, container: container, isDirectory: false, isHardLinkAlias: alias,
            isCloudOnly: false, isPending: !alias, logicalBytes: 0, allocatedBytes: 0,
            itemCount: 0, children: []
        )
    }
}

/// Orders issues by path, comparing components as a depth-first walk visits them, then
/// by message.
private func pathOrder(_ left: ScanIssue, _ right: ScanIssue) -> Bool {
    guard left.path != right.path else { return left.message < right.message }
    let components = { (path: String) in path.utf8.lazy.map { $0 == 0x2F ? 0 : $0 } }
    return components(left.path).lexicographicallyPrecedes(components(right.path))
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

private struct FileIdentity: Equatable, Hashable, Sendable {
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

    init(parent: Int32, name: String, expected: FileIdentity, isProvider: Bool) throws {
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
        guard identity == expected else {
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

private func directoryEntry(
    name: String, container: ScanLocation?, cloudOnly: Bool = false, pending: Bool = false
) -> ScanEntry {
    ScanEntry(
        name: name, container: container, isDirectory: true, isHardLinkAlias: false,
        isCloudOnly: cloudOnly, isPending: pending,
        logicalBytes: 0, allocatedBytes: 0, itemCount: 0, children: []
    )
}

private extension UInt64 {
    /// Adds an entry and the items below it. A second hard link to a file isn't an item.
    func addingCounted(_ entry: ScanEntry) -> UInt64 {
        guard !entry.isHardLinkAlias else { return self }
        let (sum, overflow) = addingReportingOverflow(entry.itemCount == .max ? .max : entry.itemCount + 1)
        return overflow ? .max : sum
    }
}

/// The folder that holds `path`, which is absolute and has no trailing separator. The
/// separator is found by byte, since a name can begin with a combining mark.
func parentPath(of path: String) -> String {
    let bytes = path.utf8
    guard let slash = bytes.lastIndex(of: 0x2F), slash != bytes.startIndex else { return "/" }
    return String(decoding: bytes[..<slash], as: UTF8.self)
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
