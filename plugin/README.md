# AXTerminator

AXTerminator lets Claude drive macOS applications through the Accessibility API. This plugin starts a local stdio MCP server by running `bin/launch.js` in this folder. That file runs npx at the pinned package version, so the axterminator binary does not already have to be on PATH. There is no hosted or remote server. Grant the macOS Accessibility permission to the app that launches Claude, then ask it to connect to a running app, click or type in that app, and take a screenshot. Screenshots stay in the session and are not sent to the author as a telemetry payload.

The same three actions cover the core loop: attach to an app that is already open, act on a control the accessibility tree can name, and capture the result to check what changed.

### Example: Connect to an app

Ask Claude to connect to a running application such as TextEdit or Safari. The ax_connect tool accepts an app name, a bundle id, or a process id. The target must already be running, and Accessibility permission must be on for the host process.

```text
Connect to TextEdit and confirm you can read its accessibility tree.
```

### Example: Click or type

Once the app is connected, ask Claude to click a control or type into a field. Clicking uses ax_click with a query such as a button title. Typing uses ax_type with the field query and the text to enter. Both act on the accessibility element instead of a guessed pixel.

```text
In TextEdit, type hello from Claude into the document, then click the Format menu.
```

### Example: Take a screenshot

Ask Claude to capture the connected app. ax_screenshot returns a PNG of the whole app, or of one element when you name it. Use that image to check that the click and the typed text landed where you expected.

```text
Take a screenshot of TextEdit and say whether the typed sentence is visible.
```

Support: Mikko Parkkola via GitHub issues at https://github.com/MikkoParkkola/axterminator/issues.
