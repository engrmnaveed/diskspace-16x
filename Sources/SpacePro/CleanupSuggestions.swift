import Foundation
import SpaceCore

/// A kind of folder that a tool recreates when it needs it.
public struct CleanupRule: Sendable, Equatable {
    enum Kind: Sendable, Equatable {
        /// A folder with this name anywhere.
        case name(String)
        /// A folder with this name beside a file with the marker's name.
        case beside(String, marker: String)
        /// A folder whose path ends with these components.
        case pathEnd(String)
    }

    /// A stable name, as `node-modules`, which the CLI's JSON uses too.
    public let id: String
    /// What the folder holds, as lists show it.
    public let label: String
    let kind: Kind
    /// For folders that can't be recreated, what removing one loses, as the warnings say
    /// it. Review folders stay out of the recreatable total.
    public var review: String?

    init(id: String, label: String, kind: Kind, review: String? = nil) {
        self.id = id
        self.label = label
        self.kind = kind
        self.review = review
    }

    /// Whether `path` is a folder this rule matches by its path alone.
    func matches(path: String) -> Bool {
        if case let .pathEnd(suffix) = kind { return path.hasSuffix("/" + suffix) }
        return false
    }
}

/// A folder the scan found that a rule matches.
public struct CleanupSuggestion: Identifiable, Sendable {
    public var id: String { entry.path }
    public let entry: ScanEntry
    public let rule: CleanupRule
}

/// Cleanup suggestions: folders such as `node_modules`, Cargo's `target` or Xcode's
/// DerivedData, found by name in a finished scan, as `diskuse16x scan --cleanup` and the CLI
/// explorer's `u` list them. It reads nothing new and removes nothing: the user picks
/// folders and confirms Move to Trash.
public enum CleanupSuggestions {
    /// The smallest folder worth suggesting, in allocated bytes.
    public static let smallestBytes: UInt64 = 1_000_000

    /// The rules, in the order they're tried. `fixtures/cleanup` holds a scan both apps test.
    public static let rules: [CleanupRule] = [
        CleanupRule(id: "xcode-derived-data", label: "Xcode build data",
                    kind: .pathEnd("Library/Developer/Xcode/DerivedData")),
        CleanupRule(id: "xcode-device-support", label: "Xcode device support",
                    kind: .pathEnd("Library/Developer/Xcode/iOS DeviceSupport")),
        CleanupRule(id: "simulator-caches", label: "Simulator caches",
                    kind: .pathEnd("Library/Developer/CoreSimulator/Caches")),
        CleanupRule(id: "xcode-archives", label: "Xcode archives",
                    kind: .pathEnd("Library/Developer/Xcode/Archives"),
                    review: "Xcode archives hold the builds you distributed and the symbols that read their crash reports. They can't be recreated."),
        CleanupRule(id: "simulator-devices", label: "Simulator devices",
                    kind: .pathEnd("Library/Developer/CoreSimulator/Devices"),
                    review: "Simulator devices hold every simulator's apps and data. They can't be recreated, and simctl delete unavailable removes only unused ones."),
        CleanupRule(id: "app-caches", label: "App caches", kind: .pathEnd("Library/Caches")),
        CleanupRule(id: "gradle-caches", label: "Gradle caches", kind: .pathEnd(".gradle/caches")),
        CleanupRule(id: "npm-cache", label: "npm cache", kind: .pathEnd(".npm/_cacache")),
        CleanupRule(id: "cache-folder", label: "Cache folder", kind: .name(".cache")),
        CleanupRule(id: "node-modules", label: "Node packages", kind: .name("node_modules")),
        CleanupRule(id: "rust-target", label: "Rust build output", kind: .beside("target", marker: "Cargo.toml")),
    ]

    /// A plain warning when `entries` are or hold folders to review before removing, which
    /// can't be recreated, such as Xcode archives, as the CLI explorer's prompt gives it.
    public static func reviewWarning(for entries: [ScanEntry]) -> String? {
        var labels: [String] = []
        var pending = entries.filter(\.isDirectory)
        while let folder = pending.popLast() {
            if let rule = rules.first(where: { $0.review != nil && $0.matches(path: folder.path) }) {
                if !labels.contains(rule.label) { labels.append(rule.label) }
            } else {
                pending.append(contentsOf: folder.children.filter(\.isDirectory))
            }
        }
        switch labels.count {
        case 0: return nil
        case 1: return "\(labels[0]) can't be recreated."
        default: return "Xcode archives and simulator devices can't be recreated."
        }
    }

    /// The rule that `child`, a folder inside `folder`, matches, if any.
    public static func rule(for child: ScanEntry, in folder: ScanEntry) -> CleanupRule? {
        guard child.isDirectory else { return nil }
        return rules.first { rule in
            switch rule.kind {
            case let .name(name):
                child.name == name
            case let .beside(name, marker):
                child.name == name && folder.children.contains { !$0.isDirectory && $0.name == marker }
            case let .pathEnd(suffix):
                child.name == suffix.split(separator: "/").last.map(String.init) && child.path.hasSuffix("/" + suffix)
            }
        }
    }

    /// Folders below `folder`, or the whole scan, that a rule matches, read completely and
    /// at least 1 MB, largest first by `metric` with ties in path order. A matched folder's
    /// contents aren't searched, so `node_modules` inside `node_modules` isn't listed again.
    public static func find(in report: ScanReport, under folder: ScanEntry? = nil, metric: SizeMetric) -> [CleanupSuggestion] {
        let issuePaths = report.issues.map(\.path)
        func isComplete(_ entry: ScanEntry) -> Bool {
            let prefix = entry.path + "/"
            return !entry.isPending && !entry.isCloudOnly
                && !issuePaths.contains { $0 == entry.path || $0.hasPrefix(prefix) }
        }
        var found: [CleanupSuggestion] = []
        var folders = [folder ?? report.root]
        while let current = folders.popLast() {
            if Task.isCancelled { return [] }
            for child in current.children where child.isDirectory {
                if let rule = rule(for: child, in: current) {
                    if child.allocatedBytes >= smallestBytes, isComplete(child) {
                        found.append(CleanupSuggestion(entry: child, rule: rule))
                    }
                } else {
                    folders.append(child)
                }
            }
        }
        return found.sorted { left, right in
            let (a, b) = (left.entry.bytes(for: metric), right.entry.bytes(for: metric))
            return a == b ? left.entry.path.utf8.lexicographicallyPrecedes(right.entry.path.utf8) : a > b
        }
    }
}
