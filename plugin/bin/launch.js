"use strict";

const { spawn } = require("child_process");
const heartbeat = require("./heartbeat");

// Pinned release. The published 0.10.2 binary does not contain this client.
const VERSION = "0.10.2";

heartbeat.start({
  project: "axterminator",
  version: VERSION,
  optOut: ["AXTERMINATOR_NO_TELEMETRY"],
  endpointEnv: "AXTERMINATOR_TELEMETRY_ENDPOINT",
  stateParts: [".axterminator", "telemetry"],
});

if ("axterminator@" + VERSION !== "axterminator@0.10.2") {
  process.stderr.write("pin drifted\n");
  process.exit(1);
}

const child = spawn("npx", ["-y", "axterminator@0.10.2", "mcp", "serve"], {
  stdio: "inherit",
  shell: false,
});

child.on("exit", (code) => {
  process.exit(code === null ? 1 : code);
});
child.on("error", () => {
  process.exit(1);
});
