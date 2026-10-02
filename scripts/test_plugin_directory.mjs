#!/usr/bin/env node
// Checks the shipped plugin/ directory bundle against the directory rules
// and the latest GitHub release. Run: node scripts/test_plugin_directory.mjs

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const pluginDir = join(root, "plugin");
const SHELLS = new Set([
  "sh",
  "bash",
  "zsh",
  "/bin/sh",
  "/bin/bash",
  "/bin/zsh",
  "/usr/bin/sh",
  "/usr/bin/bash",
  "/usr/bin/zsh",
]);

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...walk(path));
    else out.push(path);
  }
  return out;
}

function wordsOutsideFences(markdown) {
  const stripped = markdown.replace(/```[\s\S]*?```/g, " ");
  return stripped.split(/\s+/).filter((word) => /[A-Za-z0-9]/.test(word));
}

function repoPluginManifests(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    if (name === ".git" || name === "target" || name === "node_modules") continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (name === ".claude-plugin") {
        const manifest = join(path, "plugin.json");
        try {
          statSync(manifest);
          out.push(manifest);
        } catch {
          // directory without a plugin manifest
        }
      }
      out.push(...repoPluginManifests(path));
    }
  }
  return out;
}

const pluginJsons = repoPluginManifests(root);
assert.equal(pluginJsons.length, 1, `expected exactly one plugin.json, found ${pluginJsons.length}: ${pluginJsons.join(", ")}`);
assert.equal(pluginJsons[0], join(pluginDir, ".claude-plugin", "plugin.json"));

const manifest = JSON.parse(readFileSync(pluginJsons[0], "utf8"));
assert.equal(manifest.name, "axterminator");
assert.equal(manifest.license, "PolyForm-Noncommercial-1.0.0");

const response = await fetch("https://api.github.com/repos/MikkoParkkola/axterminator/releases/latest", {
  headers: {
    Accept: "application/vnd.github+json",
    "User-Agent": "axterminator-plugin-directory-test",
  },
});
assert.equal(response.ok, true, `GitHub releases/latest failed: ${response.status}`);
const release = await response.json();
const tag = release.tag_name;
assert.equal(typeof tag, "string");
const latest = tag.startsWith("v") ? tag.slice(1) : tag;
assert.equal(manifest.version, latest, `plugin version ${manifest.version} != GitHub release ${tag}`);

const readme = readFileSync(join(pluginDir, "README.md"), "utf8");
const wordCount = wordsOutsideFences(readme).length;
assert.ok(wordCount >= 40, `README has ${wordCount} words outside code fences`);
const exampleHeadings = readme.split("\n").filter((line) => line.startsWith("### Example"));
assert.ok(exampleHeadings.length >= 3, `expected >= 3 "### Example" headings, found ${exampleHeadings.length}`);

const email = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/;
const repoLicense = readFileSync(join(root, "LICENSE.md"), "utf8");
const pluginLicense = readFileSync(join(pluginDir, "LICENSE.md"), "utf8");
assert.match(pluginLicense, /PolyForm Noncommercial License 1\.0\.0/);
assert.doesNotMatch(pluginLicense, email);
for (const line of pluginLicense.split("\n")) {
  assert.ok(repoLicense.includes(line), "plugin licence line is not in the repository licence");
}
const onlyInRepo = repoLicense.split("\n").filter((line) => line.trim() && !pluginLicense.includes(line));
assert.equal(onlyInRepo.length, 1, `repo licence differs by ${onlyInRepo.length} lines`);
assert.match(onlyInRepo[0], email, "the only licence line omitted from the plugin is the contact email");

const mcp = JSON.parse(readFileSync(join(pluginDir, ".mcp.json"), "utf8"));
const servers = Object.values(mcp.mcpServers ?? {});
assert.ok(servers.length >= 1, ".mcp.json has no mcpServers");
for (const server of servers) {
  assert.equal(server.command, "node", `MCP command must be node, got ${server.command}`);
  assert.equal(SHELLS.has(server.command), false);
  assert.equal(server.url, undefined, "hosted/remote MCP url is not allowed");
  assert.deepEqual(server.args, ["${CLAUDE_PLUGIN_ROOT}/bin/launch.js"]);
  const launchPath = join(pluginDir, "bin", "launch.js");
  const launch = readFileSync(launchPath, "utf8");
  const pin = `axterminator@${latest}`;
  assert.ok(launch.includes(pin), `launch.js is missing ${pin}`);
  assert.ok(launch.includes("spawn("));
  assert.ok(launch.includes("shell: false"));
  assert.equal(launch.includes("exec("), false);
  assert.equal(launch.includes("execSync"), false);
  assert.ok(launch.includes('"mcp"'));
  assert.ok(launch.includes('"serve"'));
}

const privacy = readFileSync(join(pluginDir, "PRIVACY.md"), "utf8");
assert.match(privacy, /macOS-only/);
assert.match(privacy, /Accessibility/);
assert.match(privacy, /GitHub issues/);
assert.match(privacy, /https:\/\/github\.com\/MikkoParkkola\/axterminator\/issues/);
assert.match(privacy, /https:\/\/telemetry\.revaluator\.ai\/v1\/heartbeat/);
assert.equal(privacy.includes("telemetry.trvl.app"), false);
assert.match(privacy, /install_id/);
assert.match(privacy, /city name and country code/);
assert.match(privacy, /Coordinates and the IP are not written/);
assert.match(privacy, /AXTERMINATOR_NO_TELEMETRY/);
assert.match(privacy, /NO_TELEMETRY/);
assert.match(privacy, /DO_NOT_TRACK/);
assert.match(privacy, /The published build pinned by this folder does not\./);
assert.match(privacy, /no UI text/);
for (const path of walk(pluginDir)) {
  const text = readFileSync(path, "utf8");
  assert.doesNotMatch(text, email, `${path} contains an email address`);
}
assert.match(privacy, /Mikko Parkkola/);

console.log(`ok plugin ${manifest.version} words=${wordCount} examples=${exampleHeadings.length} release=${tag}`);
