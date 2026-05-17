// MIK-3598: WebMCP markup for axterminator public docs.
//
// Registers typed tools that AI agents (Chrome 146+ with WebMCP / Browser Run)
// can call when they visit the public docs site. Falls back gracefully on
// browsers that do not expose navigator.modelContext (no-op).
//
// Tools registered:
//   getInstallCommand(macos_version)
//   getAccessibilityPermissionGuide()
//   getMCPConfig()
//   getLatestVersion()
(function () {
  "use strict";

  if (typeof navigator === "undefined" || !navigator.modelContext) {
    // No WebMCP support — silently no-op so the page renders identically
    // in Safari, Firefox, older Chrome, and any agent visiting via vision
    // pipeline rather than typed-tool pipeline.
    return;
  }

  var mc = navigator.modelContext;

  mc.registerTool({
    name: "getInstallCommand",
    description:
      "Return the installation command for axterminator on the user's macOS. Returns the canonical Homebrew tap command; falls back to pip when macos_version < 13 (Homebrew tap requires macOS Ventura).",
    parameters: {
      macos_version: {
        type: "string",
        description: "macOS version string (e.g. '14.5', 'sonoma', '15'). Optional.",
        required: false,
      },
    },
    handler: function (args) {
      var v = (args && args.macos_version) || "";
      var major = parseInt(v.split(".")[0], 10);
      if (!isNaN(major) && major < 13) {
        return "pip install axterminator";
      }
      return "brew install MikkoParkkola/tap/axterminator";
    },
  });

  mc.registerTool({
    name: "getAccessibilityPermissionGuide",
    description:
      "Return a step-by-step guide for granting macOS Accessibility permission to axterminator. axterminator drives macOS apps via the Accessibility API and cannot run without this permission.",
    parameters: {},
    handler: function () {
      return [
        "1. Open System Settings -> Privacy & Security -> Accessibility",
        "2. Click the + button at the bottom of the app list",
        "3. Navigate to /usr/local/bin/axterminator (or wherever Homebrew installed it)",
        "4. Select the binary and click Open",
        "5. Toggle the switch next to axterminator to On",
        "6. Restart any agent or shell session that will invoke axterminator",
      ].join("\n");
    },
  });

  mc.registerTool({
    name: "getMCPConfig",
    description:
      "Return the canonical Claude Code MCP server configuration JSON snippet for adding axterminator as an MCP server. Compatible with Claude Desktop, Claude Code CLI, Cursor, and Cline.",
    parameters: {},
    handler: function () {
      return JSON.stringify(
        {
          mcpServers: {
            axterminator: {
              command: "axterminator",
              args: ["mcp"],
            },
          },
        },
        null,
        2
      );
    },
  });

  mc.registerTool({
    name: "getLatestVersion",
    description:
      "Return the latest released version of axterminator. Reads from a static field rendered at site-build time; for the truly-latest version, fetch GitHub releases instead. This tool is intended for agent-discovery latency rather than absolute freshness.",
    parameters: {},
    handler: function () {
      // The site-build pipeline can override this constant via a CI step
      // (e.g. mkdocs hook reading the latest git tag). Today it's the
      // hand-maintained baseline.
      var version = "0.x";
      var releasesUrl =
        "https://github.com/MikkoParkkola/axterminator/releases";
      return (
        "Current site-build version: " +
        version +
        ". For the absolute-latest tag, see " +
        releasesUrl
      );
    },
  });
})();
