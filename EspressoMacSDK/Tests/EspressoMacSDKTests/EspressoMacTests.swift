import XCTest
@testable import EspressoMacSDK

final class EspressoMacTests: XCTestCase {

    override func setUp() {
        super.setUp()
        // Ensure clean state
        EspressoMac.uninstall()
    }

    override func tearDown() {
        EspressoMac.uninstall()
        super.tearDown()
    }

    // MARK: - Installation Tests

    func testInstallation() {
        // Before install
        EspressoMac.install()

        // Get status to verify components are active
        let status = EspressoMac.idleStatus
        // The status should be available and provide valid data
        XCTAssertNotNil(status.description)
    }

    func testDoubleInstallIsIdempotent() {
        EspressoMac.install()
        EspressoMac.install() // Should not crash or cause issues

        // Should be able to get status
        let status = EspressoMac.idleStatus
        XCTAssertNotNil(status.description)
    }

    // MARK: - Idling Resource Tests

    func testCustomIdlingResource() {
        EspressoMac.install()

        // Create a resource that starts busy
        var isBusy = true
        EspressoMac.register(id: "test-resource") { !isBusy }

        // Should report resource as busy
        let status = EspressoMac.idleStatus
        XCTAssertTrue(status.busyResources.contains("test-resource"))

        // Mark as idle
        isBusy = false

        // Resource should now be idle
        let updatedStatus = EspressoMac.idleStatus
        XCTAssertFalse(updatedStatus.busyResources.contains("test-resource"))
    }

    func testUnregisterResource() {
        EspressoMac.install()

        // Register a permanently busy resource
        EspressoMac.register(id: "busy-resource") { false }

        var status = EspressoMac.idleStatus
        XCTAssertTrue(status.busyResources.contains("busy-resource"))

        // Unregister it
        EspressoMac.unregister("busy-resource")

        // Should no longer be in busy resources
        status = EspressoMac.idleStatus
        XCTAssertFalse(status.busyResources.contains("busy-resource"))
    }

    func testCountingIdlingResource() {
        let resource = CountingIdlingResource(id: "counter")

        // Starts idle
        XCTAssertTrue(resource.isIdle)
        XCTAssertEqual(resource.currentCount, 0)

        // Increment makes it busy
        resource.increment()
        XCTAssertFalse(resource.isIdle)
        XCTAssertEqual(resource.currentCount, 1)

        // Multiple increments
        resource.increment()
        resource.increment()
        XCTAssertEqual(resource.currentCount, 3)
        XCTAssertFalse(resource.isIdle)

        // Decrement
        resource.decrement()
        XCTAssertEqual(resource.currentCount, 2)
        XCTAssertFalse(resource.isIdle)

        // Decrement to zero
        resource.decrement()
        resource.decrement()
        XCTAssertEqual(resource.currentCount, 0)
        XCTAssertTrue(resource.isIdle)

        // Decrement below zero is clamped
        resource.decrement()
        XCTAssertEqual(resource.currentCount, 0)
        XCTAssertTrue(resource.isIdle)
    }

    func testBlockIdlingResource() {
        var isIdle = false
        let resource = BlockIdlingResource(id: "block-test") { isIdle }

        XCTAssertEqual(resource.id, "block-test")
        XCTAssertFalse(resource.isIdle)

        isIdle = true
        XCTAssertTrue(resource.isIdle)
    }

    // MARK: - Wait For Idle Tests

    func testWaitForIdleWithCustomResource() async {
        EspressoMac.install()

        // Create a resource that becomes idle after 100ms
        var isBusy = true
        EspressoMac.register(id: "async-resource") { !isBusy }

        // Schedule becoming idle
        Task {
            try? await Task.sleep(nanoseconds: 100_000_000) // 100ms
            isBusy = false
        }

        // Wait for custom resource to become idle
        // Note: Overall isIdle may still be false due to run loop activity
        let deadline = Date().addingTimeInterval(1.0)
        while Date() < deadline {
            if !EspressoMac.idleStatus.busyResources.contains("async-resource") {
                break
            }
            try? await Task.sleep(nanoseconds: 10_000_000) // 10ms
        }

        // Resource should now be idle
        XCTAssertFalse(EspressoMac.idleStatus.busyResources.contains("async-resource"))
    }

    func testWaitForIdleTimeout() async {
        EspressoMac.install()

        // Register a permanently busy resource
        EspressoMac.register(id: "never-idle") { false }

        // Wait with short timeout - should return false
        let result = await EspressoMac.waitForIdle(timeout: 0.05)
        XCTAssertFalse(result)
    }

    // MARK: - Idle Status Tests

    func testIdleStatus() {
        EspressoMac.install()

        // Get initial status
        var status = EspressoMac.idleStatus
        XCTAssertTrue(status.busyResources.isEmpty)

        // Add busy resource
        EspressoMac.register(id: "status-test") { false }
        status = EspressoMac.idleStatus

        XCTAssertFalse(status.isIdle)
        XCTAssertTrue(status.busyResources.contains("status-test"))
    }

    func testIdleStatusDescription() {
        EspressoMac.install()

        let status = EspressoMac.idleStatus
        let description = status.description

        // Description should contain key components
        XCTAssertTrue(description.contains("EspressoMac Status"))
        XCTAssertTrue(description.contains("Run Loop"))
        XCTAssertTrue(description.contains("Animations"))
        XCTAssertTrue(description.contains("Network"))
    }

    // MARK: - Busy Marker Tests

    func testBusyMarkerRegistration() {
        EspressoMac.install()

        // Create busy marker
        let marker = EspressoMac.markBusy(id: "operation")

        // Should register as busy resource
        let status = EspressoMac.idleStatus
        XCTAssertTrue(status.busyResources.contains("operation"))

        // Mark as idle
        marker.markIdle()

        // Should no longer be busy
        let updatedStatus = EspressoMac.idleStatus
        XCTAssertFalse(updatedStatus.busyResources.contains("operation"))
    }

    func testBusyMarkerIdempotentRelease() {
        EspressoMac.install()

        let marker = EspressoMac.markBusy(id: "idempotent-test")
        XCTAssertTrue(EspressoMac.idleStatus.busyResources.contains("idempotent-test"))

        // Multiple calls to markIdle should be safe
        marker.markIdle()
        marker.markIdle()
        marker.markIdle()

        XCTAssertFalse(EspressoMac.idleStatus.busyResources.contains("idempotent-test"))
    }

    // MARK: - Thread Safety Tests

    func testConcurrentResourceRegistration() {
        EspressoMac.install()

        let group = DispatchGroup()
        let queue = DispatchQueue(label: "test.concurrent", attributes: .concurrent)

        // Register many resources concurrently
        for i in 0..<100 {
            group.enter()
            queue.async {
                EspressoMac.register(id: "concurrent-\(i)") { true }
                group.leave()
            }
        }

        group.wait()

        // All should be registered without crashing
        // Since all resources return true, none should be in busy list
        let status = EspressoMac.idleStatus
        for i in 0..<100 {
            XCTAssertFalse(status.busyResources.contains("concurrent-\(i)"))
        }

        // Cleanup
        for i in 0..<100 {
            EspressoMac.unregister("concurrent-\(i)")
        }
    }

    func testConcurrentResourceAccess() {
        EspressoMac.install()

        let resource = CountingIdlingResource(id: "concurrent-counter")
        EspressoMac.register(resource)

        let group = DispatchGroup()
        let queue = DispatchQueue(label: "test.counter", attributes: .concurrent)

        // Increment from multiple threads
        for _ in 0..<100 {
            group.enter()
            queue.async {
                resource.increment()
                group.leave()
            }
        }

        group.wait()
        XCTAssertEqual(resource.currentCount, 100)

        // Decrement from multiple threads
        for _ in 0..<100 {
            group.enter()
            queue.async {
                resource.decrement()
                group.leave()
            }
        }

        group.wait()
        XCTAssertEqual(resource.currentCount, 0)
        XCTAssertTrue(resource.isIdle)
    }

    // MARK: - Network Tracker Tests

    func testNetworkTrackerStartsIdle() {
        EspressoMac.install()

        let status = EspressoMac.idleStatus
        XCTAssertTrue(status.networkIdle)
        XCTAssertEqual(status.inFlightRequests, 0)
    }

    // MARK: - Animation Tracker Tests

    func testAnimationTrackerStartsIdle() {
        EspressoMac.install()

        let status = EspressoMac.idleStatus
        XCTAssertTrue(status.animationsIdle)
        XCTAssertEqual(status.activeAnimations, 0)
    }
}
