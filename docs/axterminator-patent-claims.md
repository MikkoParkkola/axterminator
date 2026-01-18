# AXTerminator Patent Claims

**Application Date**: 2026-01-10
**Status**: Draft - Pending Legal Review
**Priority**: WORLD FIRST capabilities

---

## Patent 1: Background GUI Testing Without Focus

### Title
Method and System for Automated Graphical User Interface Testing of Desktop Applications Without Window Focus Acquisition

### Abstract
A method and system for performing automated user interface testing on desktop operating system applications without requiring the application under test to acquire window focus, thereby enabling concurrent user activity on the same computing system during test execution. The invention leverages undiscovered capabilities of accessibility APIs to perform user interface actions on unfocused application windows.

### Claims

**Claim 1**: A computer-implemented method for automated testing of graphical user interface applications comprising:
- identifying a target application executing in a background state;
- locating a user interface element within the target application through accessibility tree traversal;
- executing a user interface action on the located element using accessibility action APIs that operate on unfocused windows;
- wherein the action is performed without transferring window focus to the target application.

**Claim 2**: The method of Claim 1, wherein the accessibility action APIs comprise macOS AXUIElementPerformAction function calls that operate on AXUIElementRef references to unfocused window elements.

**Claim 3**: The method of Claim 1, further comprising:
- detecting when an action requires window focus (such as text input);
- providing fallback to focus-acquiring mode for such actions;
- restoring original focus state after focus-required actions complete.

**Claim 4**: The method of Claim 1, wherein supported background actions include: button clicks (kAXPressAction), menu selection (kAXPickAction), value increment/decrement (kAXIncrementAction, kAXDecrementAction), menu display (kAXShowMenuAction), and dialog confirmation/cancellation (kAXConfirmAction, kAXCancelAction).

**Claim 5**: A non-transitory computer-readable medium storing instructions that, when executed by a processor, perform the method of Claims 1-4.

### Prior Art Analysis
- **XCUITest**: Requires focus, activates app before testing
- **Appium Mac2**: Requires focus, uses XCUITest driver
- **Windows UI Automation**: Focus-based, no background capability
- **Playwright/Puppeteer**: Web-only, N/A for native apps

### Novelty Statement
No existing macOS GUI testing framework has discovered or documented the capability of AXUIElementPerformAction to operate on unfocused windows. This invention represents a world-first capability enabling developers to run automated tests while continuing to work on the same system.

---

## Patent 2: XPC-Based Test Synchronization

### Title
Inter-Process Idle Detection System for Deterministic GUI Test Synchronization Using XPC Protocol

### Abstract
A system and method for achieving deterministic synchronization between automated GUI test runners and applications under test through an XPC (Cross-Process Communication) protocol that exposes application idle state. The system includes an SDK installable in target applications and a test runner client that queries idle state to eliminate timing-based test flakiness.

### Claims

**Claim 1**: A synchronization system for automated testing comprising:
- a software development kit (SDK) installable in target applications that monitors idle indicators including: RunLoop activity, pending network requests, in-progress animations, and custom idling resources;
- an XPC service exposing idle state queries to external processes;
- a test runner client that queries the XPC service before proceeding with test actions.

**Claim 2**: The system of Claim 1, wherein the SDK monitors RunLoop activity by installing CFRunLoopObserver callbacks that track source signaling and timer firing.

**Claim 3**: The system of Claim 1, further comprising:
- a heuristic fallback for applications without the SDK installed;
- wherein the heuristic fallback monitors accessibility tree stability through periodic hashing and change detection.

**Claim 4**: The system of Claim 3, wherein accessibility tree stability is determined by observing N consecutive identical tree hashes over a minimum time period.

**Claim 5**: The system of Claim 1, wherein the XPC protocol defines:
- isIdle() -> Bool: immediate idle state query
- waitForIdle(timeout: TimeInterval) -> Bool: blocking wait with timeout
- registerIdlingResource(id: String, block: () -> Bool): custom idle condition registration

### Prior Art Analysis
- **Android Espresso**: Similar concept but Android-only, no macOS equivalent exists
- **XCUITest**: Basic implicit waits, no explicit idle detection
- **Polling**: All competitors use fixed delays or polling, causing flaky tests

### Novelty Statement
This is the first implementation of Espresso-style synchronization for macOS desktop applications, combining XPC for apps with SDK integration and heuristic fallback for arbitrary apps.

---

## Patent 3: Unified Cross-Technology Router

### Title
Automatic UI Technology Detection and Multi-Protocol Test Routing System for Desktop Applications

### Abstract
A routing system that automatically detects the user interface technology stack of desktop applications (native, Electron/Chromium, hybrid WebView) and applies appropriate testing protocols without manual configuration. The system enables unified test authoring across heterogeneous application types.

### Claims

**Claim 1**: A method for automated UI technology detection and test routing comprising:
- analyzing a target application to determine its UI technology stack;
- selecting an appropriate testing protocol based on detected technology;
- routing test commands through the selected protocol;
- wherein supported protocols include accessibility APIs for native apps, Chrome DevTools Protocol for Electron apps, and hybrid approaches for WebView-embedded content.

**Claim 2**: The method of Claim 1, wherein UI technology detection includes:
- checking for Chromium helper processes (indicating Electron);
- analyzing application bundle for Electron/CEF frameworks;
- detecting WKWebView or WebViewWK instances in accessibility tree (indicating hybrid);
- detecting Catalyst app markers (indicating iPad on Mac).

**Claim 3**: The method of Claim 1, wherein Electron app testing includes:
- connecting to the application's Chrome DevTools Protocol endpoint;
- executing JavaScript-based queries and actions;
- mapping CDP operations to the unified test API.

**Claim 4**: The method of Claim 1, wherein hybrid app testing includes:
- identifying WebView boundaries within native accessibility tree;
- switching protocols at WebView boundaries;
- maintaining unified element references across protocol boundaries.

**Claim 5**: A unified testing API that abstracts protocol differences, comprising:
- app(identifier) -> AppHandle: connect to any app type
- find(query) -> Element: locate elements regardless of protocol
- click(), type_text(), etc.: actions mapped to appropriate protocol

### Prior Art Analysis
- **Appium**: Supports multiple platforms but requires manual driver selection
- **Playwright**: Web-only, no native app support
- **XCUITest**: Native only, no Electron or WebView awareness

### Novelty Statement
No existing framework automatically detects and routes between native accessibility, CDP, and hybrid testing protocols within a single API.

---

## Patent 4: Multi-Attribute Self-Healing Element Locators

### Title
Cascading Element Location Strategy with Performance-Bounded Fallback and Adaptive Caching

### Abstract
A method for locating user interface elements using a priority-ordered cascade of identification strategies with strict performance budget enforcement. The system automatically heals broken test locators by trying alternative identification methods within a configurable time budget, caching successful heals for subsequent test runs.

### Claims

**Claim 1**: A method for fault-tolerant element location comprising:
- attempting element location using a primary identifier;
- upon primary failure, cascading through alternative identification strategies in priority order;
- enforcing a maximum time budget across all strategies;
- caching successful fallback mappings for future use.

**Claim 2**: The method of Claim 1, wherein identification strategies include, in priority order:
- data-testid attribute (developer-set stable identifiers);
- ARIA accessibility labels;
- native accessibility identifiers;
- element titles and labels;
- XPath structural paths;
- relative position within parent;
- visual matching using machine learning models.

**Claim 3**: The method of Claim 1, wherein performance budget enforcement includes:
- configurable maximum heal time (default 100ms);
- early termination when budget exhausted;
- strategy timeout allocation proportional to strategy reliability.

**Claim 4**: The method of Claim 1, wherein caching includes:
- storing mappings of original query to successful strategy;
- cache keyed by application version and query hash;
- cache invalidation on persistent failures.

**Claim 5**: The method of Claim 2, wherein visual matching includes:
- capturing element screenshot;
- encoding using vision-language model embeddings;
- similarity matching against reference embeddings;
- confidence threshold for match acceptance.

### Prior Art Analysis
- **Ranorex**: Basic self-healing with 1-2 strategies
- **Testim.io**: ML-based but web-only
- **Mabl**: Cloud-based healing, not local performance-bounded

### Novelty Statement
No existing solution combines 7 ordered strategies with strict performance budgets and adaptive caching for native desktop applications.

---

## Filing Recommendations

### Priority Order
1. **Patent 1 (Background Testing)**: Highest priority - WORLD FIRST, immediate commercial value
2. **Patent 2 (XPC Sync)**: High priority - novel for macOS
3. **Patent 3 (Unified Router)**: Medium priority - strong but narrower claims
4. **Patent 4 (Self-Healing)**: Medium priority - incremental over prior art

### Next Steps
1. Legal review of claims
2. Prior art search by patent attorney
3. Provisional filing within 12 months of first public use
4. PCT filing for international protection

### Commercial Protection
- Trade secret: Implementation details of background action discovery
- Copyright: Source code protection
- Trademark: "AXTerminator" name registration

---

*Document generated: 2026-01-10 | Confidence: 🟡 Legal claims need attorney review*
