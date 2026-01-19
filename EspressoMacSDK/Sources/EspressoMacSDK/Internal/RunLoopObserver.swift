import Foundation
import CoreFoundation

/// Observes the main run loop to detect idle states
final class RunLoopObserver: @unchecked Sendable {
    private var observer: CFRunLoopObserver?
    private let lock = NSLock()
    private var _isProcessing = false
    private var _lastActivityTime: CFAbsoluteTime = CFAbsoluteTimeGetCurrent()

    /// Minimum time without activity to consider idle (in seconds)
    private let idleThreshold: CFAbsoluteTime = 0.05 // 50ms

    var isIdle: Bool {
        lock.withLock {
            if _isProcessing {
                return false
            }
            let elapsed = CFAbsoluteTimeGetCurrent() - _lastActivityTime
            return elapsed >= idleThreshold
        }
    }

    var lastActivityTime: CFAbsoluteTime {
        lock.withLock { _lastActivityTime }
    }

    init() {}

    func start() {
        guard observer == nil else { return }

        // Observe all run loop activities
        let activities: CFRunLoopActivity = [
            .entry,
            .beforeTimers,
            .beforeSources,
            .beforeWaiting,
            .afterWaiting,
            .exit
        ]

        // Use unretained pointer to avoid retain cycle
        var context = CFRunLoopObserverContext(
            version: 0,
            info: Unmanaged.passUnretained(self).toOpaque(),
            retain: nil,
            release: nil,
            copyDescription: nil
        )

        observer = CFRunLoopObserverCreate(
            kCFAllocatorDefault,
            activities.rawValue,
            true,  // repeats
            0,     // order
            { _, activity, info in
                guard let info = info else { return }
                let observer = Unmanaged<RunLoopObserver>.fromOpaque(info).takeUnretainedValue()
                observer.handleActivity(activity)
            },
            &context
        )

        if let observer = observer {
            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        }
    }

    func stop() {
        if let observer = observer {
            CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes)
            self.observer = nil
        }
    }

    private func handleActivity(_ activity: CFRunLoopActivity) {
        lock.withLock {
            switch activity {
            case .beforeTimers, .beforeSources, .afterWaiting:
                // Run loop is about to process work
                _isProcessing = true
                _lastActivityTime = CFAbsoluteTimeGetCurrent()

            case .beforeWaiting:
                // Run loop is about to sleep - this means it's idle
                _isProcessing = false
                _lastActivityTime = CFAbsoluteTimeGetCurrent()

            case .exit:
                // Run loop exited
                _isProcessing = false

            default:
                break
            }
        }
    }

    deinit {
        stop()
    }
}
