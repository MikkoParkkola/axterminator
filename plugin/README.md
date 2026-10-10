# AXTerminator

This plugin runs AXTerminator in Claude Code and local Cowork. It is macOS only. Windows and Linux cannot grant the Accessibility permission the server needs, and this folder is not a Claude Desktop or remote Cowork plugin.

Load a local checkout with `claude --plugin-dir /path/to/axterminator/plugin`. The launcher is `npx`, so Node.js has to be on the PATH Claude Code uses, and npm must be allowed to run this package's install script. That script downloads the macOS binary.

AXTerminator sees and controls other Mac applications through the macOS Accessibility API. Tools stay dark until that permission is on.

On macOS Ventura and later:

1. Open System Settings.
2. Open Privacy & Security, then Accessibility.
3. Add the app that starts Claude Code, such as Terminal or the Claude app, and switch it on.
4. Run `npx -y axterminator@0.11.0 check`. The line you want is `Accessibility: OK`.

On macOS Monterey, open System Preferences, then Security & Privacy, then Privacy, then Accessibility. Unlock the pane, add the same app, and leave its box checked.

The server is the published package `axterminator@0.11.0`, started as `npx -y axterminator@0.11.0 mcp serve`. That version is pinned, so a later npm publish does not change this plugin until the manifest moves with the release.
