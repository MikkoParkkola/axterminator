# MCP directory submissions

Tracking issue: [#36](https://github.com/MikkoParkkola/axterminator/issues/36) ·
Linear: MIK-6075

axterminator is an MCP server, so it should be discoverable from the public MCP
directories. This runbook is the single source of truth for the submission
payload and the per-directory steps. Keep the payload fields below in sync with
`Cargo.toml` and `README.md`.

## Submission payload

Reuse these exact values for every directory form/field:

| Field | Value |
| --- | --- |
| Name | `axterminator` |
| Repository | `https://github.com/MikkoParkkola/axterminator` |
| Homepage | `https://mikkoparkkola.github.io/axterminator/` |
| Category | Developer tools / GUI automation (macOS) |
| Tagline | MCP server that gives AI agents the ability to see and control macOS applications. |
| Description | macOS GUI automation MCP server: background interaction via the Accessibility API, sub-millisecond element access, self-healing locators, optional vision fallback. 27+ core MCP tools. |
| License | PolyForm Noncommercial 1.0.0 |
| Install (Homebrew) | `brew install MikkoParkkola/tap/axterminator` |
| Install (crates.io) | `cargo install axterminator` |
| MCP launch command | `axterminator mcp serve` |

MCP client config snippet (for forms that ask for one):

```json
{
  "command": "axterminator",
  "args": ["mcp", "serve"]
}
```

## Glama — `glama.json` (automated, no browser)

Glama crawls public GitHub repositories and reads the repo-root
[`glama.json`](../glama.json) to claim and configure the listing. No browser or
account step is required once the file is present; the crawler picks it up and
the maintainer claim links the listing to the repository owner.

Status: **enabled in-repo.** `glama.json` is committed at the repository root
with the `MikkoParkkola` maintainer claim. The `glama_listing_metadata_is_present_and_valid`
regression test (`tests/directory_listing_tests.rs`) guards the schema and the
maintainer claim so the listing metadata cannot drift.

To verify the live listing after the next crawl, check
`https://glama.ai/mcp/servers` for `axterminator`.

## mcp.so — manual browser submission (account-gated)

mcp.so accepts servers through a browser submission form and requires a
signed-in account; it cannot be completed from an unattended/headless worker.
Steps for the operator:

1. Sign in at <https://mcp.so>.
2. Open the submit/add-server flow (<https://mcp.so/submit>).
3. Fill the form using the **Submission payload** table above.
4. Paste the MCP client config snippet when prompted for the server config.
5. Submit and record the resulting listing URL back on issue #36 / MIK-6075.

Status: **prepared, pending operator submission.** Everything the form needs is
captured above; the remaining step is the account-gated browser submission.
