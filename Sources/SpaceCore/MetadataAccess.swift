import Dispatch
import Foundation

#if canImport(Darwin)
import Darwin
#endif

struct MetadataUnavailable: LocalizedError {
    var errorDescription: String? { "The cloud provider did not return local metadata in time." }
}

struct DatalessPolicyError: LocalizedError {
    let reason: String
    var errorDescription: String? { "Could not disable cloud downloads. \(reason)" }
}

/// Known macOS provider directories can block during metadata calls even for
/// resident items. Bound the wait and the number of outstanding system calls.
final class MetadataAccess {
    private let isCancelled: @Sendable () -> Bool

    #if canImport(Darwin)
    private static let workerSlots = DispatchSemaphore(value: 2)
    private var timedOut: Set<String> = []
    #endif

    init(isCancelled: @escaping @Sendable () -> Bool) {
        self.isCancelled = isCancelled
    }

    static func isProviderPath(_ path: String) -> Bool {
        #if canImport(Darwin)
        for location in ["/Library/Mobile Documents", "/Library/CloudStorage"] {
            if path.hasSuffix(location) || path.contains(location + "/") { return true }
        }
        #endif
        return false
    }

    func perform<T: Sendable>(
        at path: String, isProvider: Bool,
        _ operation: @escaping @Sendable (_ check: () throws -> Void) throws -> T
    ) throws -> T {
        #if canImport(Darwin)
        if isProvider {
            try checkCancellation()
            guard !timedOut.contains(path), Self.workerSlots.wait(timeout: .now()) == .success else {
                throw MetadataUnavailable()
            }
            let attempt = MetadataAttempt<T>()
            let deadline = DispatchTime.now() + .seconds(1)
            let cancelled = isCancelled
            DispatchQueue.global(qos: .utility).async {
                let result = Result {
                    try withDatalessMaterializationDisabled {
                        func check() throws {
                            if attempt.isAbandoned || cancelled() { throw CancellationError() }
                        }
                        try check()
                        return try operation(check)
                    }
                }
                Self.workerSlots.signal()
                attempt.finish(result)
            }
            defer { attempt.abandon() }
            repeat {
                try checkCancellation()
                let pollUntil = min(deadline, DispatchTime.now() + .milliseconds(25))
                if attempt.ready.wait(timeout: pollUntil) == .success {
                    return try attempt.result()
                }
            } while DispatchTime.now() < deadline
            timedOut.insert(path)
            throw MetadataUnavailable()
        }
        #endif
        return try operation(checkCancellation)
    }

    private func checkCancellation() throws {
        if isCancelled() || Task.isCancelled { throw CancellationError() }
    }
}

func withDatalessMaterializationDisabled<T>(_ operation: () throws -> T) throws -> T {
    #if canImport(Darwin)
    let policy = IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES
    let previous = getiopolicy_np(policy, IOPOL_SCOPE_THREAD)
    guard previous >= 0,
          setiopolicy_np(policy, IOPOL_SCOPE_THREAD, IOPOL_MATERIALIZE_DATALESS_FILES_OFF) == 0
    else {
        throw DatalessPolicyError(reason: String(cString: strerror(errno)))
    }
    // The operation is synchronous and stays on this thread.
    defer { _ = setiopolicy_np(policy, IOPOL_SCOPE_THREAD, previous) }
    #endif
    return try operation()
}

#if canImport(Darwin)
private final class MetadataAttempt<Value: Sendable>: @unchecked Sendable {
    let ready = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var abandoned = false
    private var outcome: Result<Value, any Error>?

    var isAbandoned: Bool { lock.withLock { abandoned } }

    func abandon() {
        lock.withLock { abandoned = true }
    }

    func finish(_ result: Result<Value, any Error>) {
        lock.withLock {
            if !abandoned { outcome = result }
        }
        ready.signal()
    }

    func result() throws -> Value {
        guard let value = lock.withLock({ outcome }) else {
            throw CancellationError()
        }
        return try value.get()
    }
}
#endif
