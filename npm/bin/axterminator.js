#!/usr/bin/env node
"use strict";

// Thin launcher: exec the downloaded native binary, forwarding all arguments
// transparently so `axterminator` via npm behaves exactly like the native CLI.
// MCP clients invoke it as: { "command": "axterminator", "args": ["mcp", "serve"] }

const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");

const binaryPath = path.join(__dirname, "axterminator");

if (!fs.existsSync(binaryPath)) {
  console.error("axterminator binary not found. Re-run the install step:");
  console.error("  node " + path.join(__dirname, "install.js"));
  process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), { stdio: "inherit" });

child.on("error", (err) => {
  console.error(`Failed to start axterminator: ${err.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
