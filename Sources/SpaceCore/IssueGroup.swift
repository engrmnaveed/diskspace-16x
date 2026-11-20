/// Skipped items that share a containing folder, so hundreds of app containers collapse to one row.
public struct IssueGroup: Identifiable, Sendable {
    public let folder: String
    public let issues: [ScanIssue]
    public var id: String { folder }

    /// The item's path below the group's folder, such as `com.apple.Notes/Data`.
    public func relativePath(of issue: ScanIssue) -> String {
        let prefix = folder.hasSuffix("/") ? folder : folder + "/"
        return issue.path.hasPrefix(prefix) ? String(issue.path.dropFirst(prefix.count)) : issue.path
    }

    /// Groups items by containing folder. A folder holding a single item moves up a level when that
    /// joins it with siblings. For example, each `Containers/<app>/Data` joins one `Containers` group.
    /// Groups never rise to the scan root or to `/`, which would only repeat the scope.
    /// Larger groups come first, then by path.
    public static func grouping(_ issues: [ScanIssue], under root: String) -> [IssueGroup] {
        func canRise(_ folder: String) -> Bool {
            let next = parent(of: folder)
            return next != "/" && next != root && next.hasPrefix(root)
        }

        var folders = issues.map { parent(of: $0.path) }
        for _ in 0..<3 {
            let counts = folders.reduce(into: [String: Int]()) { $0[$1, default: 0] += 1 }
            let rising = folders.reduce(into: [String: Int]()) { result, folder in
                if counts[folder] == 1, canRise(folder) { result[parent(of: folder), default: 0] += 1 }
            }
            var changed = false
            folders = folders.map { folder in
                let next = parent(of: folder)
                guard counts[folder] == 1, canRise(folder),
                      rising[next, default: 0] + counts[next, default: 0] >= 2 else { return folder }
                changed = true
                return next
            }
            if !changed { break }
        }

        return Dictionary(grouping: zip(folders, issues), by: \.0)
            .map { folder, members in
                IssueGroup(
                    folder: folder,
                    issues: members.map(\.1).sorted { $0.path.utf8.lexicographicallyPrecedes($1.path.utf8) }
                )
            }
            .sorted {
                $0.issues.count == $1.issues.count
                    ? $0.folder.utf8.lexicographicallyPrecedes($1.folder.utf8)
                    : $0.issues.count > $1.issues.count
            }
    }

    private static func parent(of path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return path }
        return slash == path.startIndex ? "/" : String(path[..<slash])
    }
}
