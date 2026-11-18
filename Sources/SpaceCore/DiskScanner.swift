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
        var scan = Traversal(progress: progress, isCancelled: isCancelled)
        return try scan.scan(path: scope, rootName: components.last ?? "/")
    }
}

private struct Traversal {
    let progress: (@Sendable (ScanProgress) -> Void)?
    let isCancelled: @Sendable () -> Bool
    private var issues: [ScanIssue] = []
    private var hardLinks: Set<FileIdentity> = []
    private var visited: UInt64 = 0
    private var lastProgress: UInt64 = 0

    init(progress: (@Sendable (ScanProgress) -> Void)?, isCancelled: @escaping @Sendable () -> Bool) {
        self.progress = progress
        self.isCancelled = isCancelled
    }

    mutating func scan(path: String, rootName: String) throws -> ScanReport {
        try checkCancellation()
        var expected = stat()
        guard path.withCString({ lstat($0, &expected) }) == 0 else {
            throw ScanFailure(path: path, reason: posixMessage("Reading metadata", errno))
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
        let rootHandle: DirectoryHandle
        do {
            rootHandle = try DirectoryHandle(parent: AT_FDCWD, name: path, expected: expected)
        } catch let problem as DirectoryFailure {
            throw ScanFailure(path: path, reason: problem.message)
        }
        let rootListing = try rootHandle.readNames(checkCancellation: checkCancellation)
        if let error = rootListing.error {
            throw ScanFailure(path: path, reason: error)
        }

        let rootDevice = expected.st_dev
        var ancestors = Set([FileIdentity(expected)])
        var stack = [DirectoryFrame(
            path: path,
            name: rootName,
            directory: rootHandle,
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
                stack[parent].add(entry)
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

            let descriptor = stack[level].directory.descriptor
            var metadata = stat()
            guard name.displayName.withCString({ fstatat(descriptor, $0, &metadata, AT_SYMLINK_NOFOLLOW) }) == 0 else {
                issues.append(ScanIssue(path: childPath, message: posixMessage("Reading metadata", errno)))
                continue
            }
            guard metadata.st_dev == rootDevice else { continue }

            switch metadata.st_mode & mode_t(S_IFMT) {
            case mode_t(S_IFDIR):
                let identity = FileIdentity(metadata)
                guard !ancestors.contains(identity) else {
                    issues.append(ScanIssue(path: childPath, message: "Skipped a directory that points back to an ancestor."))
                    continue
                }
                do {
                    let directory = try DirectoryHandle(parent: descriptor, name: name.displayName, expected: metadata)
                    let listing = try directory.readNames(checkCancellation: checkCancellation)
                    if let error = listing.error {
                        issues.append(ScanIssue(path: childPath, message: error))
                    }
                    ancestors.insert(identity)
                    stack.append(DirectoryFrame(path: childPath, name: name.displayName, directory: directory, names: listing.names))
                } catch let problem as DirectoryFailure {
                    issues.append(ScanIssue(path: childPath, message: problem.message))
                    stack[level].add(ScanEntry(
                        path: childPath, name: name.displayName, isDirectory: true, isHardLinkAlias: false,
                        logicalBytes: 0, allocatedBytes: 0, children: []
                    ))
                }
            case mode_t(S_IFREG):
                let entry = file(name: name.displayName, path: childPath, metadata: metadata)
                stack[level].add(entry)
            default:
                // Symlinks, sockets, pipes, and device nodes have no counted bytes.
                continue
            }
        }

        preconditionFailure("Traversal must finish at the root directory.")
    }

    private mutating func file(name: String, path: String, metadata: stat) -> ScanEntry {
        let isAlias = metadata.st_nlink > 1 && !hardLinks.insert(FileIdentity(metadata)).inserted
        let logical = isAlias ? 0 : UInt64(max(0, metadata.st_size))
        // st_blocks is expressed in 512-byte units on both supported platforms.
        let blocks = isAlias ? 0 : UInt64(max(0, metadata.st_blocks))
        let (allocated, overflow) = blocks.multipliedReportingOverflow(by: 512)
        if overflow {
            issues.append(ScanIssue(path: path, message: "Allocated size exceeds the supported byte range."))
        }
        return ScanEntry(
            path: path, name: name, isDirectory: false, isHardLinkAlias: isAlias,
            logicalBytes: logical, allocatedBytes: overflow ? .max : allocated, children: []
        )
    }

    private func checkCancellation() throws {
        if isCancelled() || Task.isCancelled {
            throw CancellationError()
        }
    }

    private mutating func reportProgress(at path: String, force: Bool = false) {
        guard let progress else { return }
        let now = DispatchTime.now().uptimeNanoseconds
        guard force || now - lastProgress >= 100_000_000 else { return }
        lastProgress = now
        progress(ScanProgress(visitedEntries: visited, currentPath: path))
    }
}

private struct FileIdentity: Equatable, Hashable {
    let device: dev_t
    let inode: ino_t

    init(_ metadata: stat) {
        self.device = metadata.st_dev
        self.inode = metadata.st_ino
    }
}

private struct DirectoryName {
    let displayName: String
    let isUTF8: Bool
}

private struct DirectoryFailure: Error {
    let message: String
}

private final class DirectoryHandle {
    #if canImport(Darwin)
    private let stream: UnsafeMutablePointer<DIR>
    #else
    private let stream: OpaquePointer
    #endif
    let descriptor: Int32
    let identity: FileIdentity

    init(parent: Int32, name: String, expected: stat) throws {
        let descriptor = name.withCString { openat(parent, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC) }
        guard descriptor >= 0 else {
            throw DirectoryFailure(message: posixMessage("Opening directory", errno))
        }
        var adopted = false
        defer {
            if !adopted { _ = close(descriptor) }
        }
        var actual = stat()
        guard fstat(descriptor, &actual) == 0 else {
            throw DirectoryFailure(message: posixMessage("Reading directory metadata", errno))
        }
        let identity = FileIdentity(actual)
        guard identity == FileIdentity(expected) else {
            throw DirectoryFailure(message: "Directory changed during the scan.")
        }
        guard let stream = fdopendir(descriptor) else {
            throw DirectoryFailure(message: posixMessage("Opening directory stream", errno))
        }
        adopted = true
        self.descriptor = descriptor
        self.identity = identity
        self.stream = stream
    }

    deinit {
        _ = closedir(stream)
    }

    func readNames(checkCancellation: () throws -> Void) throws -> (names: [DirectoryName], error: String?) {
        var names: [DirectoryName] = []
        var error: String?
        while true {
            try checkCancellation()
            errno = 0
            guard let entry = readdir(stream) else {
                let code = errno
                if code != 0 { error = posixMessage("Listing directory", code) }
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

    var entry: ScanEntry {
        ScanEntry(
            path: path, name: name, isDirectory: true, isHardLinkAlias: false,
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
        let (newLogical, logicalOverflow) = logical.addingReportingOverflow(entry.logicalBytes)
        let (newAllocated, allocatedOverflow) = allocated.addingReportingOverflow(entry.allocatedBytes)
        logical = logicalOverflow ? .max : newLogical
        allocated = allocatedOverflow ? .max : newAllocated
        didOverflow = didOverflow || logicalOverflow || allocatedOverflow
        children.append(entry)
    }
}

private func posixMessage(_ operation: String, _ code: Int32) -> String {
    "\(operation) failed (\(String(cString: strerror(code))))."
}
