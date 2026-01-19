import Foundation

/// XPC protocol for communication with AXTerminator
@objc public protocol EspressoMacXPCProtocol {
    /// Check if the application is currently idle
    func isIdle(with reply: @escaping (Bool) -> Void)

    /// Wait for the application to become idle
    func waitForIdle(timeout: TimeInterval, with reply: @escaping (Bool) -> Void)

    /// Get list of registered idling resources
    func getIdlingResources(with reply: @escaping ([String]) -> Void)

    /// Get the idle state of a specific resource
    func getResourceState(id: String, with reply: @escaping (Bool) -> Void)

    /// Ping to verify connection
    func ping(with reply: @escaping (String) -> Void)
}

/// XPC server for AXTerminator to query idle state
final class XPCServer: NSObject, @unchecked Sendable {
    private var listener: NSXPCListener?
    private weak var espresso: EspressoMac?
    private let serviceName: String

    init(espresso: EspressoMac, bundleIdentifier: String) {
        self.espresso = espresso
        // Use bundle ID to create unique service name
        self.serviceName = "com.axterminator.espresso.\(bundleIdentifier)"
        super.init()
    }

    func start() {
        // Create anonymous listener for Mach service
        listener = NSXPCListener.anonymous()
        listener?.delegate = self
        listener?.resume()

        // Register the endpoint so AXTerminator can find it
        registerEndpoint()
    }

    func stop() {
        listener?.invalidate()
        listener = nil
        unregisterEndpoint()
    }

    private func registerEndpoint() {
        guard let endpoint = listener?.endpoint else { return }

        // Store endpoint in user defaults for AXTerminator discovery
        // Using a shared app group or distributed notifications
        let endpointData = try? NSKeyedArchiver.archivedData(
            withRootObject: endpoint,
            requiringSecureCoding: true
        )

        if let data = endpointData {
            // Write to a known location for AXTerminator to discover
            let tempDir = FileManager.default.temporaryDirectory
            let endpointFile = tempDir.appendingPathComponent("espresso_\(ProcessInfo.processInfo.processIdentifier).endpoint")
            try? data.write(to: endpointFile)

            // Also post a distributed notification
            DistributedNotificationCenter.default().post(
                name: Notification.Name("com.axterminator.espresso.registered"),
                object: serviceName,
                userInfo: ["pid": ProcessInfo.processInfo.processIdentifier]
            )
        }
    }

    private func unregisterEndpoint() {
        let tempDir = FileManager.default.temporaryDirectory
        let endpointFile = tempDir.appendingPathComponent("espresso_\(ProcessInfo.processInfo.processIdentifier).endpoint")
        try? FileManager.default.removeItem(at: endpointFile)

        DistributedNotificationCenter.default().post(
            name: Notification.Name("com.axterminator.espresso.unregistered"),
            object: serviceName,
            userInfo: ["pid": ProcessInfo.processInfo.processIdentifier]
        )
    }

    /// Get the XPC endpoint for direct connection
    var endpoint: NSXPCListenerEndpoint? {
        listener?.endpoint
    }
}

// MARK: - NSXPCListenerDelegate

extension XPCServer: NSXPCListenerDelegate {
    func listener(
        _ listener: NSXPCListener,
        shouldAcceptNewConnection newConnection: NSXPCConnection
    ) -> Bool {
        // Configure the connection
        newConnection.exportedInterface = NSXPCInterface(with: EspressoMacXPCProtocol.self)
        newConnection.exportedObject = XPCExportedObject(espresso: espresso)

        // Handle invalidation
        newConnection.invalidationHandler = {
            // Connection was invalidated
        }

        newConnection.resume()
        return true
    }
}

// MARK: - XPC Exported Object

private final class XPCExportedObject: NSObject, EspressoMacXPCProtocol, @unchecked Sendable {
    private weak var espresso: EspressoMac?

    init(espresso: EspressoMac?) {
        self.espresso = espresso
        super.init()
    }

    func isIdle(with reply: @escaping (Bool) -> Void) {
        reply(espresso?.isIdleInternal ?? true)
    }

    func waitForIdle(timeout: TimeInterval, with reply: @escaping (Bool) -> Void) {
        guard let espresso = espresso else {
            reply(true)
            return
        }

        Task {
            let result = await espresso.waitForIdleInternal(timeout: timeout)
            reply(result)
        }
    }

    func getIdlingResources(with reply: @escaping ([String]) -> Void) {
        let resources = espresso?.registeredResourceIds ?? []
        reply(resources)
    }

    func getResourceState(id: String, with reply: @escaping (Bool) -> Void) {
        let isIdle = espresso?.isResourceIdle(id: id) ?? true
        reply(isIdle)
    }

    func ping(with reply: @escaping (String) -> Void) {
        let version = "EspressoMac 1.0.0"
        reply(version)
    }
}
