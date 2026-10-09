# Problem definition — MIK-7617

Status: awaiting ratification. This document chooses no solution, no wire field, and no error code.

## Problem

A client that speaks MCP revision 2026-07-28 does not get that revision from axterminator. The server answers `initialize` with `protocolVersion` `"2025-11-05"`. That date is not a protocol revision. A client that speaks 2025-11-25 receives the same non-revision string.

Whose problem: the client that sends 2026-07-28, and the operator who ordered axterminator to follow nab. When: every `initialize`.

## Why now

Nab's matching ticket, MIK-7614, landed on 2026-10-09 as pull request 348, squash `345d2cbf1e17846d5e8d804fff36c4bc07fc49f0`. MIK-7617 is the next product in that order. Leaving this unsolved keeps axterminator advertising a date that is not a protocol revision.

## Decisions already made

These are asked-and-answered. They are not a design.

- 2026-10-09. The operator chose the fallback string `2025-11-25`. An `initialize` from a 2026-07-28 client and an `initialize` from a 2025-11-25 client both answer `protocolVersion` `2025-11-25`.
- 2026-10-09. The operator approved the scope in which the plugin-validate criterion stays on MIK-7618.

## Ticket text

MIK-7617, retrieved 2026-10-09. Status was Backlog. Parent is MIK-7604.

> The server speaks MCP 2025-11-25 at most (origin/main). Reuse the trvl/mcp-gateway approach.

> AC1 2026-07-28 served with 2025-11-25 fallback; conformance tests for both in CI.

> AC2 `claude plugin validate` passes on the plugin folder with no blocking finding.

The gap sentence is false on the tree named below. The server does not speak 2025-11-25. The words "reuse the trvl/mcp-gateway approach" name a mechanism. That mechanism is in the quarantine. AC1 is the requirement this problem accepts. AC2 is MIK-7618.

## Measured constraints

Read on `origin/main` `2ba9d37bb00809e50a73019458fcc58eaec157b1`, in `/Users/mikko/github/axterminator-mcp-2026`. A search of `*.rs`, `*.md`, and `*.yml` found no `2026-07-28`.

- `build_initialize_result` sets `protocol_version` to `"2025-11-05"` (`src/mcp/server_handlers.rs` line 738).
- `handle_initialize` deserializes the client params, logs `protocol_version`, and returns that result (`src/mcp/server_handlers.rs` lines 45–58). It does not compare the client string with a known revision. A client string `2026-07-28` gets a JSON-RPC success whose `protocolVersion` is `2025-11-05`, when the params otherwise deserialize.
- `tools/list` is dispatched only while `phase == Running` (`src/mcp/server.rs` line 184). Before that, the same method returns a JSON-RPC error, "Server not yet initialized" (lines 218–223). `notifications/initialized` moves the phase from `Initializing` to `Running` (lines 240–243). `handle_tools_list` does not read a protocol revision (`src/mcp/server_handlers.rs` lines 78–88). After the phase is `Running`, `tools/list` succeeds for any client version the initialize handler accepted.
- Stdio and HTTP share that server. HTTP holds one `ServerHandle` across requests (`src/mcp/transport.rs` lines 183–188). The HTTP entry is `axterminator mcp serve --http <port>` (`src/bin/axterminator.rs` lines 212–213).
- Tests lock the current string. `src/mcp/transport.rs` line 516 asserts the initialize result equals `"2025-11-05"`. `src/mcp/server_tests.rs` sends that string as the client `protocolVersion` at lines 32, 65, 85, 220, 239, 258, 281, 510, 814, 837, and 859.
- `read_system_status` writes `"protocol_version": "2025-11-05"` into a resource payload (`src/mcp/resources_read.rs` line 32). The acceptance signal below does not name this resource.
- `docs/design/MCP_SERVER_DESIGN.md` line 4 says the protocol is MCP 2025-11-05. The acceptance signal below does not name this document.
- The protocol types are hand-rolled (`src/mcp/protocol.rs`). The crate does not depend on `rust-mcp-sdk`.
- `elicit_ambiguous_app`, `elicit_element_not_found`, `elicit_destructive_action`, and `elicit_permissions_missing` are called from `src/mcp/elicitation.rs` and `src/mcp/elicitation_tests.rs`. No other file calls them. `sampling::create_message` is called from its own test (`src/mcp/sampling.rs` line 542). No tool handler calls it. `ax_find_visual` returns a tool result. When sampling is available that result includes a suggested `sampling/createMessage` payload (`src/mcp/tools_handlers.rs` lines 496–527). The handler does not write that request to the client.
- CI job `test` runs on `macos-latest` and executes the locked test command with `--all-features` and `--skip live_` (`.github/workflows/ci.yml` lines 42–64). There is no separate protocol job.
- No `plugin.json` exists in the tree.
- The checkout `/Users/mikko/github/axterminator` is a different worktree and stays on its own branch. This change does not commit there.

## Acceptance signal

All three are true on the revision under review. Each one that names a version string is false on `2ba9d37`.

1. An `initialize` whose client `protocolVersion` is `2026-07-28`, and an `initialize` whose client `protocolVersion` is `2025-11-25`, each return a JSON-RPC success whose result `protocolVersion` is `2025-11-25`. The result does not contain `2025-11-05` in that field. The same result holds on stdio and on HTTP, because both transports use the one server.
2. Once the server is in the phase where it answers `tools/list` today, `tools/list` returns a JSON-RPC success for the client from item 1 that sent `2026-07-28` and for the client that sent `2025-11-25`. On `2ba9d37` this is already true after `notifications/initialized`, because `tools/list` does not read the revision. Item 2 alone does not show that 2026-07-28 is served. Item 1 is the check that fails today.
3. The existing CI `test` job runs a test that fails on `2ba9d37` for item 1, for both client versions, and that test passes on the revision under review. A job that still asserts `"2025-11-05"` does not satisfy item 1.

A `tools/call` that finishes without the server sending `elicitation/create` or `sampling/createMessage` keeps today's outcome kind: a tool result, or a tool error. A `tools/call` that would send either request returns an error and does not send that request. On `2ba9d37` no tool handler sends either request, so no current call takes that error branch. `ax_find_visual` stays a tool result. This paragraph does not name an error code.

## Exclusions

- MIK-7604, the Claude directory submission.
- MIK-7618, a plugin folder and a pinned launcher. AC2 stays there. This tree has no `plugin.json`.
- MIK-7438, the vision-fallback documentation mismatch.
- The nab SDK patch, the `nab_sdk_patch` cfg, and the crates.io build that does not serve 2026-07-28. This crate has no such SDK.
- Clock examples in `src/mcp/security.rs` whose text contains `2025-11-05`. Those are timestamps.
- Publishing a crate, moving a tag, and pushing this branch. None of those is approved by this document.

## Quarantine

Named and set aside. None of these is the answer this document selects.

- The ticket's gap sentence tells the change to reuse the trvl and mcp-gateway approach: a stateless per-request field, `server/discover`, a header check, and a 2025-11-25 fallback. The fallback date is the operator's decision above. The field, the method, and the header check are not selected here.
- Nab's landed change serves a 2026-07-28 call that does not wait, refuses a call that would wait, and answers `initialize` with `2025-11-25`. It does that with a vendored SDK patch that a published crate strips. That mechanism is nab's. It is not selected here.

## Where the work sits

The statement lives at `docs/design/mcp-2026-07-28-problem.md` on `feat/mcp-2026-07-28`, cut from `origin/main` `2ba9d37`. Status stays "awaiting ratification" until both reviews record an approval. A later solution document stays empty until that approval is recorded.
