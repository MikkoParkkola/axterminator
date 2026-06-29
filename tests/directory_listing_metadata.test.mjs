import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const manifest = JSON.parse(readFileSync(new URL("../glama.json", import.meta.url), "utf8"));
const handoff = readFileSync(
  new URL("../docs/directory-submissions.md", import.meta.url),
  "utf8",
);

test("Glama manifest contains public listing metadata", () => {
  /*
  - [ ] Glama listing created
  */
  assert.equal(manifest.$schema, "https://glama.ai/mcp/schemas/server.json");
  assert.equal(manifest.name, "axterminator");
  assert.equal(manifest.displayName, "AXTerminator");
  assert.equal(manifest.repository.url, "https://github.com/MikkoParkkola/axterminator");
  assert.deepEqual(manifest.startCommand.args, ["mcp", "serve"]);
  assert.match(manifest.description, /macOS GUI automation MCP server/);
  assert.ok(manifest.categories.includes("macos"));
  assert.ok(manifest.maintainers.includes("MikkoParkkola"));
});

test("mcp.so submission packet records owner account blocker", () => {
  /*
  - [ ] [mcp.so](<http://mcp.so>) submission sent
  */
  assert.match(handoff, /\| mcp\.so \| Blocked pending Mikko's browser\/account access\. \|/);
  assert.match(handoff, /\| Name \| AXTerminator \|/);
  assert.match(handoff, /\| Repository \| https:\/\/github\.com\/MikkoParkkola\/axterminator \|/);
  assert.match(handoff, /\| Install \| `brew install MikkoParkkola\/tap\/axterminator` \|/);
  assert.match(handoff, /\| Start command \| `axterminator mcp serve` \|/);
  assert.match(handoff, /https:\/\/github\.com\/MikkoParkkola\/axterminator\/issues\/36/);
});

test("directory submission handoff tracks review and follow-up contract", () => {
  /*
  - [ ] MIK.AXTE.1 — Root cause identified and a fix implemented for the issue described below; change is reviewed, merged to main, and deployed to production.
  - [ ] MIK.AXTE.2 — A regression test (or reproducible verification step) covers the fixed behavior and passes in CI.
  - [ ] MIK.AXTE.3 — The originating GitHub issue is referenced/closed once the fix is merged to main and deployed to production.
  */
  assert.match(handoff, /## Conclusion/);
  assert.match(handoff, /## Follow-up/);
  assert.match(
    handoff,
    /N\/A: this handoff adds directory listing metadata and an owner-account submission\npacket only; it does not add a runtime action, event, metric, or audit signal\./,
  );
  assert.match(handoff, /GitHub issue #36/);
  assert.match(
    handoff,
    /Close GitHub issue #36 only after both public directory entries are live\./,
  );
});
