import Foundation

/// Exclude patterns: a small gitignore-like subset, matched on UTF-8 path bytes.
///
/// - `*` matches any run of characters within one path segment, and `?` one character.
/// - A `**` segment matches zero or more whole segments.
/// - A leading `/` anchors the pattern to the scan root. Otherwise it matches at any
///   depth, so `node_modules` or `*.log` match an entry with that name anywhere and
///   `build/cache` matches those two segments anywhere.
/// - A trailing `/` matches directories only.
///
/// Keep this behavior in sync with `cli/src/scanner/exclude.rs`. Both test suites read
/// `fixtures/exclude-patterns.json`.
public struct ExcludeRules: Sendable, Equatable {
    public struct InvalidPattern: LocalizedError, Sendable, Equatable {
        public let pattern: String
        public var errorDescription: String? {
            "The exclude pattern \(pattern.debugDescription) matches nothing."
        }
    }

    /// The patterns as given, without blank ones or trailing whitespace.
    public let sources: [String]
    private let patterns: [Pattern]

    public var isEmpty: Bool { patterns.isEmpty }

    public init() {
        sources = []
        patterns = []
    }

    /// Compiles patterns, ignoring blank ones. Throws for a pattern that can never match.
    public init(patterns: [String]) throws {
        var sources: [String] = []
        var compiled: [Pattern] = []
        for source in patterns {
            let trimmed = String(source.reversed().drop(while: \.isWhitespace).reversed())
            guard !trimmed.isEmpty else { continue }
            compiled.append(try Pattern(trimmed))
            sources.append(trimmed)
        }
        self.sources = sources
        self.patterns = compiled
    }

    /// Splits a patterns file or settings text into patterns, one per line. Blank lines and
    /// lines starting with `#` are skipped, as `space16x --exclude-from` does.
    public static func patterns(fromLines text: String) -> [String] {
        text.split(whereSeparator: \.isNewline)
            .map { String($0.reversed().drop(while: \.isWhitespace).reversed()) }
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
    }

    /// Whether a pattern that applies to any entry type matches. Check this before
    /// reading metadata, so excluded paths cost nothing and report no errors.
    func matchesAnyType(relative: [UInt8], name: [UInt8]) -> Bool {
        patterns.contains { !$0.directoriesOnly && $0.matches(relative: relative, name: name) }
    }

    /// Whether a directories-only pattern matches a directory.
    func matchesDirectory(relative: [UInt8], name: [UInt8]) -> Bool {
        patterns.contains { $0.directoriesOnly && $0.matches(relative: relative, name: name) }
    }
}

private let globstar = Array("**".utf8)
private let slash: UInt8 = 0x2F
private let star: UInt8 = 0x2A
private let question: UInt8 = 0x3F

private struct Pattern: Sendable, Equatable {
    let segments: [[UInt8]]
    /// Unanchored, single-segment patterns compare only the entry name.
    let nameOnly: Bool
    let directoriesOnly: Bool

    init(_ source: String) throws {
        var body = Substring(source)
        let directoriesOnly = body.hasSuffix("/")
        if directoriesOnly { body = body.dropLast() }
        let anchored = body.hasPrefix("/")
        if anchored { body = body.dropFirst() }
        var segments = body.split(separator: "/").map { Array($0.utf8) }
        guard !segments.isEmpty else { throw ExcludeRules.InvalidPattern(pattern: source) }
        let nameOnly = !anchored && segments.count == 1 && segments[0] != globstar
        if !anchored && !nameOnly {
            segments.insert(globstar, at: 0)
        }
        self.segments = segments
        self.nameOnly = nameOnly
        self.directoriesOnly = directoriesOnly
    }

    func matches(relative: [UInt8], name: [UInt8]) -> Bool {
        if nameOnly {
            return segmentMatches(segments[0], name)
        }
        let path = relative.split(separator: slash, omittingEmptySubsequences: false).map(Array.init)
        return segmentsMatch(segments[...], path[...])
    }
}

private func segmentsMatch(_ pattern: ArraySlice<[UInt8]>, _ path: ArraySlice<[UInt8]>) -> Bool {
    guard let first = pattern.first else { return path.isEmpty }
    let rest = pattern.dropFirst()
    if first == globstar {
        return (path.startIndex...path.endIndex).contains { segmentsMatch(rest, path[$0...]) }
    }
    guard let name = path.first else { return false }
    return segmentMatches(first, name) && segmentsMatch(rest, path.dropFirst())
}

/// Matches one segment with `*` and `?`, backtracking only to the most recent `*`.
/// A `**` inside a segment, such as `cache**`, behaves like `*`.
private func segmentMatches(_ pattern: [UInt8], _ name: [UInt8]) -> Bool {
    var p = 0
    var n = 0
    var lastStar: (pattern: Int, name: Int)?
    while n < name.count {
        if p < pattern.count, pattern[p] == star {
            lastStar = (p, n)
            p += 1
        } else if p < pattern.count, pattern[p] == question {
            p += 1
            n += characterWidth(name[n])
        } else if p < pattern.count, pattern[p] == name[n] {
            p += 1
            n += 1
        } else if let resumeFrom = lastStar {
            let resume = resumeFrom.name + characterWidth(name[resumeFrom.name])
            lastStar = (resumeFrom.pattern, resume)
            p = resumeFrom.pattern + 1
            n = resume
        } else {
            return false
        }
    }
    return pattern[min(p, pattern.count)...].allSatisfy { $0 == star }
}

/// The byte length of the UTF-8 character starting with `lead`, so `?` and `*` step over
/// whole characters. Bytes that aren't valid UTF-8 lead bytes count as one character.
private func characterWidth(_ lead: UInt8) -> Int {
    switch lead {
    case 0xC0...0xDF: 2
    case 0xE0...0xEF: 3
    case 0xF0...0xF7: 4
    default: 1
    }
}
