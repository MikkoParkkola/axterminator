import Foundation

/// Tracks in-flight network requests via URLSession
final class NetworkTracker: IdlingResource, @unchecked Sendable {
    public let id = "espresso.network"

    private let lock = NSLock()
    private var _inFlightTasks: Set<Int> = []  // Task identifiers
    private var isObserving = false

    public var isIdle: Bool {
        lock.withLock { _inFlightTasks.isEmpty }
    }

    var inFlightCount: Int {
        lock.withLock { _inFlightTasks.count }
    }

    init() {}

    func start() {
        guard !isObserving else { return }
        isObserving = true
        swizzleURLSessionMethods()
    }

    func stop() {
        guard isObserving else { return }
        isObserving = false
    }

    // MARK: - Task Tracking

    func trackTask(_ task: URLSessionTask) {
        lock.lock()
        _inFlightTasks.insert(task.taskIdentifier)
        lock.unlock()
    }

    func taskCompleted(_ taskIdentifier: Int) {
        lock.lock()
        _inFlightTasks.remove(taskIdentifier)
        lock.unlock()
    }

    // MARK: - Swizzling

    // Use nonisolated(unsafe) for swizzling state - protected by swizzleLock
    nonisolated(unsafe) private static var swizzled = false
    private static let swizzleLock = NSLock()

    private func swizzleURLSessionMethods() {
        NetworkTracker.swizzleLock.lock()
        defer { NetworkTracker.swizzleLock.unlock() }

        guard !NetworkTracker.swizzled else { return }
        NetworkTracker.swizzled = true

        // Swizzle resume() to catch all task types
        swizzleTaskResume()
    }

    private func swizzleTaskResume() {
        guard let originalMethod = class_getInstanceMethod(URLSessionTask.self, #selector(URLSessionTask.resume)) else { return }
        guard let swizzledMethod = class_getInstanceMethod(URLSessionTask.self, #selector(URLSessionTask.espresso_resume)) else { return }

        method_exchangeImplementations(originalMethod, swizzledMethod)
    }
}

// MARK: - URLSessionTask Extension for Swizzling

// Associated object keys - nonisolated(unsafe) for objc_getAssociatedObject
nonisolated(unsafe) private var espressoTrackedKey: UInt8 = 0
nonisolated(unsafe) private var espressoObservationKey: UInt8 = 1

extension URLSessionTask {
    private var isEspressoTracked: Bool {
        get { objc_getAssociatedObject(self, &espressoTrackedKey) as? Bool ?? false }
        set { objc_setAssociatedObject(self, &espressoTrackedKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }

    @objc dynamic func espresso_resume() {
        // Track this task if not already tracked and EspressoMac is installed
        if !isEspressoTracked {
            let tracker = EspressoMac.shared.networkTracker
            isEspressoTracked = true
            tracker.trackTask(self)

            // Observe completion via KVO
            let taskId = taskIdentifier
            let observation = observe(\.state, options: [.new]) { [weak tracker] _, change in
                if let state = change.newValue, state == .completed || state == .canceling {
                    tracker?.taskCompleted(taskId)
                }
            }

            // Store observation to prevent deallocation
            objc_setAssociatedObject(
                self,
                &espressoObservationKey,
                observation,
                .OBJC_ASSOCIATION_RETAIN_NONATOMIC
            )
        }

        // Call original resume (swizzled, so this calls the real resume)
        espresso_resume()
    }
}
