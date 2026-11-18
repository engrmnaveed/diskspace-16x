import Foundation
import SpaceCore

#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

@main
enum SpaceCLI {
    private static let version = "0.1.1"
    private static let usage = """
        Usage: space16x scan PATH [--top N] [--metric allocated|logical] [--json]

        Scan a local directory using filesystem metadata.
          --top N         Number of ranked files to show (default: 20, use 0 for totals only)
          --metric SIZE   Ranking metric (default: allocated)
          --json          Write a versioned JSON report to standard output
          --              Treat the following argument as the path, even if it starts with a dash
          -h, --help      Show this help
          --version       Show the installed version

        Exit codes: 0 complete, 1 fatal or usage error, 2 partial results
        """

    static func main() {
        do {
            exit(try run(Array(CommandLine.arguments.dropFirst())))
        } catch let error as UsageError {
            writeError("space16x: \(error.message)\nRun 'space16x --help' for usage.\n")
            exit(1)
        } catch {
            writeError("space16x: \(error.localizedDescription)\n")
            exit(1)
        }
    }

    private static func run(_ arguments: [String]) throws -> Int32 {
        switch arguments {
        case ["--help"], ["-h"], ["scan", "--help"], ["scan", "-h"]:
            try write(usage + "\n")
            return 0
        case ["--version"]:
            try write("space16x \(version)\n")
            return 0
        default:
            break
        }
        guard arguments.first == "scan" else {
            throw UsageError(message: arguments.isEmpty ? "Choose a command." : "Unknown command \(arguments[0].debugDescription).")
        }

        let options = try ScanOptions(arguments.dropFirst())
        let scan = try DiskScanner.scan(path: options.path)
        let files = scan.largestFiles(limit: options.top, by: options.metric)
        if options.json {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            var data = try encoder.encode(JSONReport(scan: scan, options: options, topFiles: files))
            data.append(0x0A)
            try FileHandle.standardOutput.write(contentsOf: data)
        } else {
            try writeTextReport(scan, files: files, metric: options.metric)
        }
        return scan.isPartial ? 2 : 0
    }

    private static func writeTextReport(_ scan: ScanReport, files: [ScanEntry], metric: SizeMetric) throws {
        let status = scan.isPartial ? "Partial (some paths were skipped or could not be counted)" : "Complete"
        let totals = """
            Scan: \(scan.root.path.debugDescription)
            Status: \(status)
            Allocated: \(scan.root.allocatedBytes) bytes
            Logical:   \(scan.root.logicalBytes) bytes

            """
        try write(totals)
        if !files.isEmpty {
            try write("\nLargest files by \(metric.rawValue) size\nAllocated (bytes)\tLogical (bytes)\tPath\n")
            for file in files {
                try write("\(file.allocatedBytes)\t\(file.logicalBytes)\t\(file.path.debugDescription)\n")
            }
        }
        for issue in scan.issues {
            writeError("space16x: \(issue.path.debugDescription): \(issue.message)\n")
        }
    }

    private static func write(_ string: String) throws {
        try FileHandle.standardOutput.write(contentsOf: Data(string.utf8))
    }

    private static func writeError(_ string: String) {
        try? FileHandle.standardError.write(contentsOf: Data(string.utf8))
    }
}

private struct UsageError: Error {
    let message: String
}

private struct ScanOptions {
    let path: String
    private(set) var top = 20
    private(set) var metric: SizeMetric = .allocated
    private(set) var json = false

    init(_ arguments: ArraySlice<String>) throws {
        var remaining = arguments
        var optionsEnded = false
        var path: String?
        while let argument = remaining.popFirst() {
            switch argument {
            case "--" where !optionsEnded:
                optionsEnded = true
            case "--top" where !optionsEnded:
                guard let value = remaining.popFirst(), let limit = Int(value), limit >= 0 else {
                    throw UsageError(message: "--top requires a nonnegative integer.")
                }
                top = limit
            case "--metric" where !optionsEnded:
                guard let value = remaining.popFirst(), let metric = SizeMetric(rawValue: value) else {
                    throw UsageError(message: "--metric requires allocated or logical.")
                }
                self.metric = metric
            case "--json" where !optionsEnded:
                json = true
            default:
                if !optionsEnded, argument.hasPrefix("-") {
                    throw UsageError(message: "Unknown option \(argument.debugDescription).")
                }
                guard path == nil else {
                    throw UsageError(message: "Provide exactly one directory path.")
                }
                path = argument
            }
        }
        guard let path, !path.isEmpty else {
            throw UsageError(message: "Provide a directory path.")
        }
        self.path = path
    }
}

private struct JSONReport: Encodable {
    let schemaVersion = 1
    let scope: String
    let metric: SizeMetric
    let topLimit: Int
    let logicalBytes: UInt64
    let allocatedBytes: UInt64
    let isPartial: Bool
    let issues: [ScanIssue]
    let topFiles: [FileResult]

    init(scan: ScanReport, options: ScanOptions, topFiles: [ScanEntry]) {
        self.scope = scan.root.path
        self.metric = options.metric
        self.topLimit = options.top
        self.logicalBytes = scan.root.logicalBytes
        self.allocatedBytes = scan.root.allocatedBytes
        self.isPartial = scan.isPartial
        self.issues = scan.issues
        self.topFiles = topFiles.map(FileResult.init)
    }

    struct FileResult: Encodable {
        let path: String
        let logicalBytes: UInt64
        let allocatedBytes: UInt64

        init(_ entry: ScanEntry) {
            self.path = entry.path
            self.logicalBytes = entry.logicalBytes
            self.allocatedBytes = entry.allocatedBytes
        }
    }
}
