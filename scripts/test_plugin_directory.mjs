#!/usr/bin/env node
// Checks the shipped plugin/ directory bundle against the directory rules
// and the latest GitHub release. Run: node scripts/test_plugin_directory.mjs

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, join } from "node:path";
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

const pluginJsons = walk(pluginDir).filter((path) => basename(path) === "plugin.json");
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

const repoLicense = readFileSync(join(root, "LICENSE.md"));
const pluginLicense = readFileSync(join(pluginDir, "LICENSE.md"));
assert.ok(repoLicense.equals(pluginLicense), "plugin/LICENSE.md bytes differ from LICENSE.md");

const mcp = JSON.parse(readFileSync(join(pluginDir, ".mcp.json"), "utf8"));
const servers = Object.values(mcp.mcpServers ?? {});
assert.ok(servers.length >= 1, ".mcp.json has no mcpServers");
for (const server of servers) {
  assert.equal(server.command, "npx", `MCP command must be npx, got ${server.command}`);
  assert.equal(SHELLS.has(server.command), false);
  assert.equal(server.url, undefined, "hosted/remote MCP url is not allowed");
  assert.ok(Array.isArray(server.args), "MCP args must be an array");
  assert.ok(server.args.includes("axterminator@0.10.2"), `missing axterminator@0.10.2 in ${JSON.stringify(server.args)}`);
  assert.deepEqual(server.args, ["-y", "axterminator@0.10.2", "mcp", "serve"]);
}

const privacy = readFileSync(join(pluginDir, "PRIVACY.md"), "utf8");
assert.match(privacy, /macOS-only/);
assert.match(privacy, /Accessibility/);
assert.match(privacy, /GitHub issues/);
assert.match(privacy, /https:\/\/github\.com\/MikkoParkkola\/axterminator\/issues/);
assert.doesNotMatch(privacy, /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/);
assert.equal(privacy.includes("@"), false, "PRIVACY.md must not contain @");

console.log(`ok plugin ${manifest.version} words=${wordCount} examples=${exampleHeadings.length} release=${tag}`);
