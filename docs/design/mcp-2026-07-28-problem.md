# Problem definition — MIK-7617

Status: awaiting ratification. Second statement. This document chooses no solution, no parser, and no library.

The first statement's acceptance could pass while a client that speaks only 2026-07-28 still could not use the server. This statement corrects that. It also corrects measured claims the first review checked against the tree.

## Problem

Axterminator answers `initialize` with `protocolVersion` `"2025-11-05"`. That date is not a protocol revision. The current specification's revision list, read 2026-10-09, is `2024-11-05`, `2025-03-26`, `2025-06-18`, `2025-11-25`, and `2026-07-28` ([versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)).

A client on 2026-07-28 does not open with `initialize`. The same page says there is no negotiation handshake. Every request carries its protocol version. In that page's matrix, a modern client against a legacy-only server fails. Axterminator only answers the legacy `initialize` path, and it answers that path with a non-revision. A legacy client that speaks `2025-11-25` receives the same non-revision string.

Whose problem: the modern client, the legacy client that speaks `2025-11-25`, and the operator who ordered axterminator to follow nab. When: every connection from either client.

## Why now

Nab's matching ticket, MIK-7614, landed on 2026-10-09 as pull request 348, squash `345d2cbf1e17846d5e8d804fff36c4bc07fc49f0`. MIK-7617 is the next product in that order. Leaving this unsolved keeps the matrix row "modern client, legacy server" on this server, and keeps a legacy client on a version string that is not a revision.

## Decisions already made

Asked and answered in this chat on 2026-10-09. They are not a Linear comment and they are not a design.

- The legacy fallback string is `2025-11-25`. An `initialize` from a client that sends `2026-07-28` in `protocolVersion`, and an `initialize` from a client that sends `2025-11-25`, both answer `protocolVersion` `2025-11-25`.
- The plugin-validate criterion stays on MIK-7618.

## Ticket text

MIK-7617, retrieved 2026-10-09. Status was Backlog. Parent is MIK-7604.

> The server speaks MCP 2025-11-25 at most (origin/main). Reuse the trvl/mcp-gateway approach.

> AC1 2026-07-28 served with 2025-11-25 fallback; conformance tests for both in CI.

> AC2 `claude plugin validate` passes on the plugin folder with no blocking finding.

AC1 is the requirement. AC2 is MIK-7618. The gap sentence overshoots what `initialize` does: the result string is `2025-11-05`, not `2025-11-25`. Comments and the design document do name `2025-11-25`. See the measured constraints. "Reuse the trvl/mcp-gateway approach" names someone else's code. That reuse is quarantined. The specification's own requirements for a 2026-07-28 server are constraints, listed below, and they are not a choice of that code.

## Measured constraints

Read on `origin/main` `2ba9d37bb00809e50a73019458fcc58eaec157b1`, in `/Users/mikko/github/axterminator-mcp-2026`. A search of `*.rs`, `*.md`, and `*.yml` found no `2026-07-28`.

- `build_initialize_result` sets `protocol_version` to `"2025-11-05"` (`src/mcp/server_handlers.rs` line 738).
- `handle_initialize` deserializes the client params, logs the client string, and returns that result (`src/mcp/server_handlers.rs` lines 45–58). It does not compare the client string with a known revision.
- `tools/list` runs only while `phase == Running` (`src/mcp/server.rs` line 184). Before that, the method returns a JSON-RPC error, "Server not yet initialized" (lines 218–223). `notifications/initialized` moves the phase to `Running` (lines 240–243). `handle_tools_list` does not read a protocol revision (`src/mcp/server_handlers.rs` lines 78–88).
- Stdio constructs `Server::new()` (`src/mcp/server.rs` line 328). HTTP constructs a separate `ServerHandle::new()` (`src/mcp/transport.rs` line 207). Both are instances of the same server implementation. They are not one shared object.
- `src/mcp/transport.rs` line 516 asserts the initialize result equals `"2025-11-05"`. `src/mcp/server_tests.rs` sends that string as the client `protocolVersion` at lines 32, 65, 85, 220, 239, 258, 281, 510, 814, 837, and 859.
- `read_system_status` writes `"protocol_version": "2025-11-05"` (`src/mcp/resources_read.rs` line 32). `docs/design/MCP_SERVER_DESIGN.md` line 1704 shows that payload as `"2025-11-25"`. The resource and that example disagree on `2ba9d37`.
- `docs/design/MCP_SERVER_DESIGN.md` line 4 says the protocol is MCP 2025-11-05. Section 3.1 (lines 595–597) says the target is 2025-11-25, that the server negotiates the client's version, and that it falls back to `2025-06-18` and `2025-03-26`. `handle_initialize` does not do that negotiation.
- `src/mcp/protocol.rs` line 1 says the types are MCP 2025-11-25, wire-compatible with 2025-11-05. `src/mcp/annotations.rs` line 3 cites MCP 2025-11-25 for tool hints. Those comments are not the string `initialize` returns.
- The protocol types are hand-rolled. The crate does not depend on `rust-mcp-sdk`.
- No match arm handles `server/discover` (`src/mcp/server.rs` lines 180–234). An unknown method returns method-not-found.
- `elicit_ambiguous_app`, `elicit_element_not_found`, `elicit_destructive_action`, and `elicit_permissions_missing` are called from `src/mcp/elicitation.rs` and `src/mcp/elicitation_tests.rs` only. `sampling::create_message` is called from its own test (`src/mcp/sampling.rs` line 542). No tool handler calls it. `ax_find_visual` returns a tool result and, when sampling is available, includes a suggested `sampling/createMessage` payload (`src/mcp/tools_handlers.rs` lines 496–527). It does not write that request.
- CI job `test` runs on `macos-latest` with `--all-features` and `--skip live_` (`.github/workflows/ci.yml` lines 42–64).
- No `plugin.json` exists in the tree.
- The checkout `/Users/mikko/github/axterminator` is a different worktree. This change does not commit there.
- This artifact adopts no third-party code. A later solution that copies code from trvl or mcp-gateway checks that code's license before the copy.

The 2026-07-28 versioning page, read 2026-10-09, states these as requirements of a server that implements that revision. They are in scope as obligations. This document does not choose the code that meets them.

- Every request declares its protocol version. On HTTP the version is also carried in `MCP-Protocol-Version`.
- A requested version the server does not implement gets `UnsupportedProtocolVersionError`. The page shows code `-32022` and a `supported` list.
- Servers implement `server/discover`. Clients may call it and are not required to.
- A server may implement both the modern per-request behavior and the legacy `initialize` behavior. AC1 asks for that pair: 2026-07-28 served, with a 2025-11-25 fallback.

## Acceptance signal

Baseline on `2ba9d37`, separate from the pass condition:

- Item 1 fails. Both client strings receive `protocolVersion` `2025-11-05`.
- Item 2 already holds. After `notifications/initialized`, `tools/list` succeeds and does not read the revision. Item 2 is a non-regression. It does not show that a modern client is served.
- Item 3 is unmet. No test asserts that the initialize result is `2025-11-25`.
- Item 4 fails. A `tools/list` before the phase is `Running` returns "Server not yet initialized", and nothing reads a per-request version.
- Item 5 fails. `server/discover` is method-not-found.

On the revision under review, all five hold.

1. An `initialize` whose client `protocolVersion` is `2026-07-28`, and an `initialize` whose client `protocolVersion` is `2025-11-25`, each return a JSON-RPC success whose result `protocolVersion` is `2025-11-25`. That field is not `2025-11-05`. Stdio and HTTP each show this result.
2. Once the phase is `Running`, `tools/list` still returns a JSON-RPC success for both clients in item 1.
3. The CI `test` job runs a test that fails on `2ba9d37` for item 1, for both client strings, and that test passes on the revision under review. The sentence is about the initialize result's `protocolVersion`. `src/mcp/security.rs` asserts a timestamp that contains the characters `2025-11-05`. That assertion is not item 3, and item 3 does not require it to change.
4. A modern `tools/list`, as the versioning page defines a modern request, with the requested version `2026-07-28`, returns a JSON-RPC success. It does not require a prior `initialize`. On HTTP, a request whose header and per-request version disagree is not this success. This item does not choose the status code of that disagreement.
5. A modern `server/discover` returns a JSON-RPC success whose supported versions include `2026-07-28` and `2025-11-25`.

When item 1 changes the advertised string, `read_system_status` and `MCP_SERVER_DESIGN.md` line 4 no longer agree with that string. The same change updates both so neither still states that the server speaks `2025-11-05`. The first review added this sentence. The earlier scope's acceptance signal did not name them.

## Open questions

Each one blocks a solution that depends on it. Neither is answered by items 1 through 5.

| question | owner | what resolves it | when | fallback if unanswered |
|---|---|---|---|---|
| What does `initialize` return for a client string other than `2026-07-28` and `2025-11-25`, including `2025-06-18`, `2025-03-26`, `2024-11-05`, `2025-11-05`, and an unknown string? Design §3.1 describes an echo the code does not perform. | operator | the operator's answer | before the solution review | one result builder still returns `2025-11-25` for every client string that deserializes. The echo in §3.1 is not selected |
| On a modern request, may the server send `elicitation/create` or `sampling/createMessage`? The nab rule, assumed when the scope was approved, says a call that would send either returns an error and does not send it. That rule was not re-asked for axterminator. No tool handler sends either request today. | operator | the operator's answer | before the solution review | the solution adds no new wait and invents no refusal code |

## Exclusions

- MIK-7604, the Claude directory submission.
- MIK-7618, a plugin folder and a pinned launcher. AC2 stays there. This tree has no `plugin.json`.
- MIK-7438, the vision-fallback documentation mismatch.
- Clock examples in `src/mcp/security.rs` whose text contains `2025-11-05`.
- Publishing a crate, moving a tag, and pushing this branch. None of those is approved by this document.

## Quarantine

Named and set aside. Not selected.

- Copying the trvl or mcp-gateway implementation, and copying nab's vendored SDK patch. This crate has no `rust-mcp-sdk`. The specification obligations above stay in scope without that code.
- Nab's rule that a waiting call returns a JSON-RPC error. It sits in the open question. It is not an acceptance check.

## Where the work sits

The statement lives at `docs/design/mcp-2026-07-28-problem.md` on `feat/mcp-2026-07-28`, cut from `origin/main` `2ba9d37`. Status stays "awaiting ratification" until both reviews record an approval of this statement. The solution document stays empty until that approval is recorded.
