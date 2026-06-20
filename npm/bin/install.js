#!/usr/bin/env node
"use strict";

// Postinstall: download the platform-matching axterminator binary from the
// matching GitHub release and verify it against the published SHA-256 checksum
// before writing it to disk. axterminator drives the macOS Accessibility API,
// so this package is macOS-only by design.

const https = require("https");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const VERSION = require("../package.json").version;
const REPO = "MikkoParkkola/axterminator";

function target() {
  if (process.platform !== "darwin") {
    throw new Error(
      `axterminator is macOS-only (it drives the macOS Accessibility API). ` +
        `Current platform: ${process.platform}.`
    );
  }
  if (process.arch === "arm64") return "aarch64-apple-darwin";
  if (process.arch === "x64") return "x86_64-apple-darwin";
  throw new Error(
    `Unsupported macOS architecture: ${process.arch} (need arm64 or x64).`
  );
}

function download(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        if (
          res.statusCode >= 300 &&
          res.statusCode < 400 &&
          res.headers.location
        ) {
          return download(res.headers.location).then(resolve).catch(reject);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} from ${url}`));
        }
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

function sha256(buf) {
  return crypto.createHash("sha256").update(buf).digest("hex");
}

async function install() {
  let triple;
  try {
    triple = target();
  } catch (err) {
    console.error(`\n${err.message}\n`);
    process.exit(1);
  }

  const assetName = `axterminator-${triple}`;
  const base = `https://github.com/${REPO}/releases/download/v${VERSION}`;
  const binDir = __dirname;
  const binaryPath = path.join(binDir, "axterminator");

  if (fs.existsSync(binaryPath)) {
    console.log(`axterminator binary already present at ${binaryPath}`);
    return;
  }

  console.log(`Downloading axterminator v${VERSION} (${triple})...`);
  console.log(`  ${base}/${assetName}`);

  let binary, checksums;
  try {
    [binary, checksums] = await Promise.all([
      download(`${base}/${assetName}`),
      download(`${base}/checksums-sha256.txt`),
    ]);
  } catch (err) {
    console.error(`\nFailed to download axterminator:\n  ${err.message}\n`);
    console.error(
      `Download manually from:\n  https://github.com/${REPO}/releases/tag/v${VERSION}\n`
    );
    process.exit(1);
  }

  // Verify the binary against the published checksum before trusting it.
  const expectedLine = checksums
    .toString("utf8")
    .split("\n")
    .find((l) => l.includes(assetName));
  if (!expectedLine) {
    console.error(
      `No checksum entry for ${assetName} in checksums-sha256.txt — refusing to install.`
    );
    process.exit(1);
  }
  const expected = expectedLine.trim().split(/\s+/)[0];
  const actual = sha256(binary);
  if (expected !== actual) {
    console.error(
      `Checksum mismatch for ${assetName}:\n  expected ${expected}\n  actual   ${actual}\n` +
        `Refusing to install a binary that does not match the published checksum.`
    );
    process.exit(1);
  }

  fs.writeFileSync(binaryPath, binary);
  fs.chmodSync(binaryPath, 0o755);
  console.log(
    `axterminator v${VERSION} installed and verified (sha256 ${actual.slice(0, 12)}…).`
  );
}

install();
