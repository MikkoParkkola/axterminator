import Foundation
import QuartzCore

/// Tracks active Core Animation animations
final class AnimationTracker: @unchecked Sendable {
    private let lock = NSLock()
    private var _activeAnimations: Set<String> = []
    private var isObserving = false

    var isIdle: Bool {
        lock.withLock { _activeAnimations.isEmpty }
    }

    var activeAnimationCount: Int {
        lock.withLock { _activeAnimations.count }
    }

    init() {}

    func start() {
        guard !isObserving else { return }
        isObserving = true

        // Observe CATransaction completions
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(transactionDidComplete),
            name: NSNotification.Name("CATransactionCompletedNotification"),
            object: nil
        )

        // Swizzle CAAnimation to track animations
        swizzleAnimationMethods()
    }

    func stop() {
        guard isObserving else { return }
        isObserving = false

        NotificationCenter.default.removeObserver(self)
    }

    @objc private func transactionDidComplete(_ notification: Notification) {
        // Clear completed animations after transaction
        DispatchQueue.main.async { [weak self] in
            self?.checkForCompletedAnimations()
        }
    }

    private func checkForCompletedAnimations() {
        // This is called periodically to clean up finished animations
        // In production, animations are tracked via swizzling
    }

    // MARK: - Animation Tracking

    func trackAnimation(_ animation: CAAnimation, key: String?) {
        let identifier = key ?? UUID().uuidString
        lock.lock()
        _activeAnimations.insert(identifier)
        lock.unlock()

        // Set up completion handler
        let originalDelegate = animation.delegate
        animation.delegate = AnimationDelegateProxy(
            tracker: self,
            identifier: identifier,
            originalDelegate: originalDelegate
        )
    }

    func animationDidStop(_ identifier: String) {
        lock.lock()
        _activeAnimations.remove(identifier)
        lock.unlock()
    }

    // MARK: - Swizzling

    // Use nonisolated(unsafe) for swizzling state - protected by swizzleLock
    nonisolated(unsafe) private static var swizzled = false
    private static let swizzleLock = NSLock()

    private func swizzleAnimationMethods() {
        AnimationTracker.swizzleLock.lock()
        defer { AnimationTracker.swizzleLock.unlock() }

        guard !AnimationTracker.swizzled else { return }
        AnimationTracker.swizzled = true

        // Swizzle CALayer.add(_:forKey:)
        guard let originalMethod = class_getInstanceMethod(
            CALayer.self,
            #selector(CALayer.add(_:forKey:))
        ) else { return }

        guard let swizzledMethod = class_getInstanceMethod(
            CALayer.self,
            #selector(CALayer.espresso_add(_:forKey:))
        ) else { return }

        method_exchangeImplementations(originalMethod, swizzledMethod)
    }

    deinit {
        stop()
    }
}

// MARK: - Animation Delegate Proxy

private class AnimationDelegateProxy: NSObject, CAAnimationDelegate {
    weak var tracker: AnimationTracker?
    let identifier: String
    weak var originalDelegate: CAAnimationDelegate?

    init(tracker: AnimationTracker, identifier: String, originalDelegate: CAAnimationDelegate?) {
        self.tracker = tracker
        self.identifier = identifier
        self.originalDelegate = originalDelegate
        super.init()
    }

    func animationDidStart(_ anim: CAAnimation) {
        originalDelegate?.animationDidStart?(anim)
    }

    func animationDidStop(_ anim: CAAnimation, finished flag: Bool) {
        tracker?.animationDidStop(identifier)
        originalDelegate?.animationDidStop?(anim, finished: flag)
    }
}

// MARK: - CALayer Extension for Swizzling

extension CALayer {
    @objc dynamic func espresso_add(_ animation: CAAnimation, forKey key: String?) {
        // Track this animation if EspressoMac is installed
        let tracker = EspressoMac.shared.animationTracker
        tracker.trackAnimation(animation, key: key)

        // Call original implementation (which is now espresso_add due to swizzle)
        espresso_add(animation, forKey: key)
    }
}
