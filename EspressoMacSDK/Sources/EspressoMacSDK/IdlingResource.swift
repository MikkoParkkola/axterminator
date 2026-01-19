import Foundation

/// Protocol for custom idling resources.
///
/// Implement this protocol to register custom conditions that the test framework
/// should wait for before considering the app idle.
///
/// Example:
/// ```swift
/// class DatabaseSyncResource: IdlingResource {
///     var id: String { "database-sync" }
///     var isIdle: Bool { !database.isSyncing }
/// }
/// ```
public protocol IdlingResource: AnyObject {
    /// Unique identifier for this resource
    var id: String { get }

    /// Returns true when this resource is idle and ready for interaction
    var isIdle: Bool { get }
}

/// A simple block-based idling resource for quick registration
public final class BlockIdlingResource: IdlingResource, @unchecked Sendable {
    public let id: String
    private let idleCheck: () -> Bool

    public var isIdle: Bool {
        idleCheck()
    }

    public init(id: String, isIdle: @escaping () -> Bool) {
        self.id = id
        self.idleCheck = isIdle
    }
}

/// A counting semaphore-style idling resource
///
/// Useful for tracking multiple in-flight operations:
/// ```swift
/// let operationResource = CountingIdlingResource(id: "operations")
/// operationResource.increment() // Start operation
/// operationResource.decrement() // End operation
/// ```
public final class CountingIdlingResource: IdlingResource, @unchecked Sendable {
    public let id: String
    private let lock = NSLock()
    private var count: Int = 0

    public var isIdle: Bool {
        lock.withLock { count == 0 }
    }

    public init(id: String) {
        self.id = id
    }

    public func increment() {
        lock.withLock { count += 1 }
    }

    public func decrement() {
        lock.withLock { count = max(0, count - 1) }
    }

    public var currentCount: Int {
        lock.withLock { count }
    }
}
