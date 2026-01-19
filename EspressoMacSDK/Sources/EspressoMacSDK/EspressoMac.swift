import Foundation
import QuartzCore

/// EspressoMac - Test synchronization SDK for macOS applications
///
/// Provides automatic idle detection for reliable UI testing with AXTerminator.
///
/// ## Quick Start
/// ```swift
/// // In your AppDelegate or App init:
/// EspressoMac.install()
/// ```
///
/// ## Custom Idling Resources
/// ```swift
/// class DatabaseResource: IdlingResource {
///     var id: String { "database" }
///     var isIdle: Bool { !database.isSyncing }
/// }
///
/// EspressoMac.register(DatabaseResource())
/// ```
public final class EspressoMac: @unchecked Sendable {
    // MARK: - Singleton

    /// Shared instance
    public static let shared = EspressoMac()

    // MARK: - Internal Components

    internal let runLoopObserver = RunLoopObserver()
    internal let animationTracker = AnimationTracker()
    internal let networkTracker = NetworkTracker()
    private var xpcServer: XPCServer?

    // MARK: - Resource Management

    private let resourceLock = NSLock()
    private var customResources: [String: IdlingResource] = [:]

    // MARK: - State

    private var isInstalled = false
    private let installLock = NSLock()

    // MARK: - Configuration

    /// Polling interval for idle checks (in seconds)
    public var pollingInterval: TimeInterval = 0.01 // 10ms

    /// Maximum time to wait for idle before timeout (in seconds)
    public var defaultTimeout: TimeInterval = 30.0

    // MARK: - Initialization

    private init() {}

    // MARK: - Public API

    /// Install EspressoMac - call once during app initialization
    ///
    /// This sets up all automatic idle detection including:
    /// - Run loop monitoring
    /// - Animation tracking
    /// - Network request tracking
    /// - XPC server for AXTerminator communication
    ///
    /// Example:
    /// ```swift
    /// @main
    /// struct MyApp: App {
    ///     init() {
    ///         EspressoMac.install()
    ///     }
    ///     var body: some Scene { ... }
    /// }
    /// ```
    public static func install() {
        shared.performInstall()
    }

    /// Uninstall EspressoMac (primarily for testing)
    public static func uninstall() {
        shared.performUninstall()
    }

    /// Register a custom idling resource
    ///
    /// Use this to add custom conditions that must be satisfied
    /// before the app is considered idle.
    ///
    /// - Parameter resource: The idling resource to register
    public static func register(_ resource: IdlingResource) {
        shared.registerResource(resource)
    }

    /// Register a block-based idling resource
    ///
    /// Convenience method for simple idle checks:
    /// ```swift
    /// EspressoMac.register(id: "my-operation") {
    ///     return !isOperationInProgress
    /// }
    /// ```
    ///
    /// - Parameters:
    ///   - id: Unique identifier for this resource
    ///   - isIdle: Block that returns true when idle
    public static func register(id: String, isIdle: @escaping () -> Bool) {
        shared.registerResource(BlockIdlingResource(id: id, isIdle: isIdle))
    }

    /// Unregister an idling resource by ID
    ///
    /// - Parameter id: The ID of the resource to unregister
    public static func unregister(_ id: String) {
        shared.unregisterResource(id)
    }

    /// Check if the application is currently idle
    ///
    /// Returns true when:
    /// - Run loop is not processing events
    /// - No animations are in progress
    /// - No network requests are in flight
    /// - All custom idling resources report idle
    public static var isIdle: Bool {
        shared.isIdleInternal
    }

    /// Wait for the application to become idle
    ///
    /// - Parameter timeout: Maximum time to wait (defaults to 30 seconds)
    /// - Returns: true if idle was reached, false if timeout occurred
    public static func waitForIdle(timeout: TimeInterval? = nil) async -> Bool {
        await shared.waitForIdleInternal(timeout: timeout ?? shared.defaultTimeout)
    }

    /// Get detailed idle status for debugging
    public static var idleStatus: IdleStatus {
        shared.getIdleStatus()
    }

    // MARK: - Internal Implementation

    private func performInstall() {
        installLock.lock()
        defer { installLock.unlock() }

        guard !isInstalled else { return }
        isInstalled = true

        // Start all trackers
        runLoopObserver.start()
        animationTracker.start()
        networkTracker.start()

        // Register network tracker as a resource
        registerResource(networkTracker)

        // Start XPC server
        let bundleId = Bundle.main.bundleIdentifier ?? "unknown"
        xpcServer = XPCServer(espresso: self, bundleIdentifier: bundleId)
        xpcServer?.start()

        #if DEBUG
        print("[EspressoMac] Installed for bundle: \(bundleId)")
        #endif
    }

    private func performUninstall() {
        installLock.lock()
        defer { installLock.unlock() }

        guard isInstalled else { return }
        isInstalled = false

        runLoopObserver.stop()
        animationTracker.stop()
        networkTracker.stop()
        xpcServer?.stop()

        resourceLock.lock()
        customResources.removeAll()
        resourceLock.unlock()

        #if DEBUG
        print("[EspressoMac] Uninstalled")
        #endif
    }

    private func registerResource(_ resource: IdlingResource) {
        resourceLock.lock()
        customResources[resource.id] = resource
        resourceLock.unlock()
    }

    private func unregisterResource(_ id: String) {
        resourceLock.lock()
        customResources.removeValue(forKey: id)
        resourceLock.unlock()
    }

    internal var isIdleInternal: Bool {
        // Check run loop
        guard runLoopObserver.isIdle else { return false }

        // Check animations
        guard animationTracker.isIdle else { return false }

        // Check all custom resources (including network tracker)
        let resources = resourceLock.withLock { Array(customResources.values) }
        for resource in resources {
            if !resource.isIdle {
                return false
            }
        }

        return true
    }

    internal func waitForIdleInternal(timeout: TimeInterval) async -> Bool {
        let deadline = Date().addingTimeInterval(timeout)

        while Date() < deadline {
            if isIdleInternal {
                // Double-check after a small delay to ensure stability
                try? await Task.sleep(nanoseconds: UInt64(pollingInterval * 1_000_000_000))
                if isIdleInternal {
                    return true
                }
            }

            // Wait before checking again
            try? await Task.sleep(nanoseconds: UInt64(pollingInterval * 1_000_000_000))
        }

        return false
    }

    internal var registeredResourceIds: [String] {
        resourceLock.withLock { Array(customResources.keys) }
    }

    internal func isResourceIdle(id: String) -> Bool {
        resourceLock.withLock {
            customResources[id]?.isIdle ?? true
        }
    }

    private func getIdleStatus() -> IdleStatus {
        let resources = resourceLock.withLock { customResources }
        var busyResources: [String] = []

        for (id, resource) in resources {
            if !resource.isIdle {
                busyResources.append(id)
            }
        }

        return IdleStatus(
            isIdle: isIdleInternal,
            runLoopIdle: runLoopObserver.isIdle,
            animationsIdle: animationTracker.isIdle,
            networkIdle: networkTracker.isIdle,
            activeAnimations: animationTracker.activeAnimationCount,
            inFlightRequests: networkTracker.inFlightCount,
            busyResources: busyResources
        )
    }
}

// MARK: - Idle Status

/// Detailed idle status for debugging
public struct IdleStatus: Sendable {
    public let isIdle: Bool
    public let runLoopIdle: Bool
    public let animationsIdle: Bool
    public let networkIdle: Bool
    public let activeAnimations: Int
    public let inFlightRequests: Int
    public let busyResources: [String]

    public var description: String {
        """
        EspressoMac Status:
          Overall: \(isIdle ? "IDLE" : "BUSY")
          Run Loop: \(runLoopIdle ? "idle" : "processing")
          Animations: \(animationsIdle ? "idle" : "\(activeAnimations) active")
          Network: \(networkIdle ? "idle" : "\(inFlightRequests) in-flight")
          Busy Resources: \(busyResources.isEmpty ? "none" : busyResources.joined(separator: ", "))
        """
    }
}

// MARK: - Convenience Extensions

extension EspressoMac {
    /// Create a scoped busy indicator that automatically marks idle when released
    ///
    /// Usage:
    /// ```swift
    /// func performOperation() async {
    ///     let busy = EspressoMac.markBusy(id: "my-operation")
    ///     defer { busy.markIdle() }
    ///
    ///     // ... do work ...
    /// }
    /// ```
    public static func markBusy(id: String) -> BusyMarker {
        BusyMarker(id: id)
    }

    /// Busy marker that can be released to indicate idle
    public final class BusyMarker: @unchecked Sendable {
        private let id: String
        private let resource: CountingIdlingResource
        private var released = false
        private let lock = NSLock()

        init(id: String) {
            self.id = id
            self.resource = CountingIdlingResource(id: id)
            resource.increment()
            EspressoMac.register(resource)
        }

        /// Mark this operation as idle/complete
        public func markIdle() {
            lock.lock()
            defer { lock.unlock() }

            guard !released else { return }
            released = true
            resource.decrement()
            if resource.currentCount == 0 {
                EspressoMac.unregister(id)
            }
        }

        deinit {
            markIdle()
        }
    }
}
