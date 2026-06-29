# Directory Submissions

Tracking artifact for GitHub issue
[#36](https://github.com/MikkoParkkola/axterminator/issues/36) and Linear issue
MIK-6075.

## Status

| Directory | Status | Evidence |
| --- | --- | --- |
| Glama | Listing metadata created in [`glama.json`](../glama.json). | The manifest includes the repository URL, homepage, description, categories, maintainer, and stdio start command. |
| mcp.so | Blocked pending Mikko's browser/account access. | mcp.so requires manual browser submission from the project owner account. This isolated worker must not open account flows or trigger prompts. |

## mcp.so Submission Packet

Use this payload when submitting through the mcp.so browser form:

| Field | Value |
| --- | --- |
| Name | AXTerminator |
| Slug | axterminator |
| Website | https://mikkoparkkola.github.io/axterminator/ |
| Repository | https://github.com/MikkoParkkola/axterminator |
| Package | https://crates.io/crates/axterminator |
| Summary | MCP server that gives AI agents the ability to see and control macOS applications through the Accessibility API. |
| Description | AXTerminator exposes macOS GUI automation as MCP tools: inspect accessibility trees, find UI elements, click controls, type text, capture screenshots, run AppleScript, audit accessibility, and control apps in the background while the user keeps working. |
| Install | `brew install MikkoParkkola/tap/axterminator` |
| Start command | `axterminator mcp serve` |
| MCP config | `{"mcpServers":{"axterminator":{"command":"axterminator","args":["mcp","serve"]}}}` |
| Categories | macOS, GUI automation, accessibility, testing, agent tools |
| License | AXTerminator Community License + commercial license |
| Source issue | https://github.com/MikkoParkkola/axterminator/issues/36 |

## Glama Listing Packet

The repository now carries the Glama manifest at [`glama.json`](../glama.json).
The listing should be created from that file by Glama's crawler or by uploading
the manifest in the Glama submission flow from Mikko's account.

## Telemetry

N/A: this handoff adds directory listing metadata and an owner-account submission
packet only; it does not add a runtime action, event, metric, or audit signal.

## Conclusion

The repository-side Glama listing metadata is complete and machine-checked. The
mcp.so external submission cannot be truthfully marked sent from this isolated
worker because the ticket explicitly requires Mikko's browser and accounts.

## Follow-up

Intended follow-up title: Complete owner-account directory submissions for axterminator

Intended follow-up summary:

Submit AXTerminator to mcp.so using the packet in
`docs/directory-submissions.md`, then verify the Glama listing is visible from
the public Glama directory or submit `glama.json` manually from Mikko's account.
Close GitHub issue #36 only after both public directory entries are live.
