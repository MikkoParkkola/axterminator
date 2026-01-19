# EspressoMac SDK

Test synchronization SDK for macOS applications. Provides automatic idle detection for reliable UI testing with AXTerminator.

Inspired by Android's Espresso testing framework.

## Quick Start

1-line integration in your app:

```swift
import EspressoMacSDK

@main
struct MyApp: App {
    init() {
        EspressoMac.install()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
```

That's it. EspressoMac automatically tracks:
- **Run loop activity** - Detects when the app is processing events
- **Animations** - Tracks Core Animation in progress
- **Network requests** - Monitors URLSession tasks in flight

## Custom Idling Resources

For app-specific busy states, register custom idling resources:

```swift
// Block-based (simple)
EspressoMac.register(id: "database-sync") {
    return !database.isSyncing
}

// Protocol-based (reusable)
class DatabaseResource: IdlingResource {
    var id: String { "database" }
    var isIdle: Bool { !database.isSyncing }
}

EspressoMac.register(DatabaseResource())
```

### Counting Resource

For tracking multiple concurrent operations:

```swift
let operationCounter = CountingIdlingResource(id: "operations")

// Start operations
operationCounter.increment()
operationCounter.increment()

// Complete operations
operationCounter.decrement()
operationCounter.decrement()

// isIdle returns true when count reaches 0
```

### Busy Marker

Scoped busy indicator with automatic cleanup:

```swift
func performOperation() async {
    let busy = EspressoMac.markBusy(id: "my-operation")
    defer { busy.markIdle() }

    // ... do work ...
}
```

## API Reference

### EspressoMac

| Method | Description |
|--------|-------------|
| `install()` | Initialize EspressoMac (call once at app start) |
| `uninstall()` | Stop EspressoMac (primarily for testing) |
| `register(_:)` | Register an IdlingResource |
| `register(id:isIdle:)` | Register a block-based resource |
| `unregister(_:)` | Unregister resource by ID |
| `isIdle` | Check if app is currently idle |
| `waitForIdle(timeout:)` | Async wait for idle state |
| `idleStatus` | Get detailed idle status |
| `markBusy(id:)` | Create a BusyMarker |

### IdleStatus

```swift
let status = EspressoMac.idleStatus
print(status.isIdle)           // Overall idle state
print(status.runLoopIdle)      // Run loop idle
print(status.animationsIdle)   // Animations idle
print(status.networkIdle)      // Network idle
print(status.activeAnimations) // Animation count
print(status.inFlightRequests) // Network request count
print(status.busyResources)    // IDs of busy resources
```

## How It Works

### Run Loop Observer
Monitors CFRunLoop to detect when the app is processing events vs waiting for input.

### Animation Tracker
Swizzles `CALayer.add(_:forKey:)` to track Core Animation lifecycle via delegate proxies.

### Network Tracker
Swizzles `URLSessionTask.resume()` and observes task state via KVO to track in-flight requests.

### XPC Server
Exposes idle state to AXTerminator via XPC for test synchronization.

## Requirements

- macOS 12.0+
- Swift 5.9+

## Installation

### Swift Package Manager

```swift
dependencies: [
    .package(path: "../EspressoMacSDK")
]
```

## Performance

- Idle check: ~1ms
- Memory overhead: Minimal
- No polling when idle

## License

MIT
