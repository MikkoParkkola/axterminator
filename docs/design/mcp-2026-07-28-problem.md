# Problem definition — MIK-7617

Status: awaiting ratification. This document chooses no solution, no parser, and no library.

## Problem

Axterminator answers `initialize` with `protocolVersion` `"2025-11-05"`. That date is not a protocol revision. The [2026-07-28 versioning page](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), read 2026-10-09, does not list revisions. It calls `2026-07-28` a modern revision and calls `2025-11-25` and earlier legacy. The [2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog) names `2025-11-25` as the previous revision. The [2025-11-25 changelog](https://modelcontextprotocol.io/specification/2025-11-25/changelog) names `2025-06-18` as the previous revision. `2025-11-05` is in neither chain.

A client on `2026-07-28` does not open with `initialize`. The versioning page says there is no negotiation handshake. Every request carries its protocol version in `_meta` under `io.modelcontextprotocol/protocolVersion`. In that page's matrix, a modern client against a legacy-only server fails. Axterminator only answers the legacy `initialize` path, and it answers that path with a non-revision. A legacy client that speaks `2025-11-25` receives the same non-revision string.

Whose problem: the modern client, the legacy client that speaks `2025-11-25`, and the operator who ordered axterminator to follow nab. When: every connection from either client.

## Why now

Nab's matching ticket, MIK-7614, landed on 2026-10-09 as pull request 348, squash `345d2cbf1e17846d5e8d804fff36c4bc07fc49f0`. The operator ordered MIK-7617 next. MIK-7618 has no plugin folder in this tree. MIK-7604 is the directory submission and stays out. MIK-7438 is a documentation mismatch elsewhere. Leaving this unsolved keeps the matrix row "modern client, legacy server" on this server, and keeps a legacy client on a version string that is not a revision.

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

Read on `origin/main` `2ba9d37bb00809e50a73019458fcc58eaec157b1`, in `/Users/mikko/github/axterminator-mcp-2026`. On that parent, a search of `*.rs`, `*.md`, and `*.yml` found no `2026-07-28`.

- `build_initialize_result` sets `protocol_version` to `"2025-11-05"` (`src/mcp/server_handlers.rs` line 738).
- `handle_initialize` deserializes the client params, logs the client string, and returns that result (`src/mcp/server_handlers.rs` lines 45–58). It does not compare the client string with a known revision.
- A new server starts in `Phase::Uninitialized` (`src/mcp/server.rs` line 135).
- `initialize` and `ping` are answered in every phase (`src/mcp/server.rs` lines 181–182).
- `tools/list` runs only while `phase == Running` (`src/mcp/server.rs` line 184). Any other method, while the phase is not `Running`, returns a JSON-RPC error, "Server not yet initialized" (lines 218–223). The method-not-found arm runs only once the phase is `Running` (lines 225–233). `notifications/initialized` moves `Initializing` to `Running` (lines 240–243). `handle_tools_list` does not read a protocol revision (`src/mcp/server_handlers.rs` lines 78–88).
- Stdio constructs `Server::new()` (`src/mcp/server.rs` line 328). HTTP constructs a separate `ServerHandle::new()` (`src/mcp/transport.rs` line 207). Both are instances of the same server implementation. They are not one shared object.
- `src/mcp/transport.rs` line 516 asserts the initialize result equals `"2025-11-05"`. `src/mcp/server_tests.rs` sends that string as the client `protocolVersion` at lines 32, 65, 85, 220, 239, 258, 281, 510, 814, 837, and 859.
- `read_system_status` writes `"protocol_version": "2025-11-05"` (`src/mcp/resources_read.rs` line 32). `docs/design/MCP_SERVER_DESIGN.md` line 1704 shows that payload as `"2025-11-25"`. The resource and that example disagree on `2ba9d37`.
- `docs/design/MCP_SERVER_DESIGN.md` line 4 says the protocol is MCP 2025-11-05. Section 3.1 (lines 595–597) says the target is 2025-11-25, that the server negotiates the client's version, and that it falls back to `2025-06-18` and `2025-03-26`. `handle_initialize` does not do that negotiation.
- `src/mcp/protocol.rs` line 1 says the types are MCP 2025-11-25, wire-compatible with 2025-11-05. `src/mcp/annotations.rs` line 3 cites MCP 2025-11-25 for tool hints. Those comments are not the string `initialize` returns.
- The protocol types are hand-rolled. The crate does not depend on `rust-mcp-sdk`.
- No match arm handles `server/discover` (`src/mcp/server.rs` lines 180–234).
- `elicit_ambiguous_app`, `elicit_element_not_found`, `elicit_destructive_action`, and `elicit_permissions_missing` are called from `src/mcp/elicitation.rs` and `src/mcp/elicitation_tests.rs` only. `sampling::create_message` is called from its own test (`src/mcp/sampling.rs` line 542). No tool handler calls it. `ax_find_visual` returns a tool result and, when sampling is available, includes a suggested `sampling/createMessage` payload (`src/mcp/tools_handlers.rs` lines 496–527). It does not write that request. `client_supports_sampling` is set only in `handle_initialize` (`src/mcp/server_handlers.rs` line 56). It defaults to false (`src/mcp/server.rs` line 140) and is read at `src/mcp/server_handlers.rs` line 555. A modern request that never calls `initialize` does not pass through that write.
- CI job `test` runs on `macos-latest` with `--all-features` and `--skip live_` (`.github/workflows/ci.yml` lines 42–64).
- No `plugin.json` exists in the tree.
- The checkout `/Users/mikko/github/axterminator` is a different worktree. This change does not commit there.
- This artifact adopts no third-party code. A later solution that copies code from trvl or mcp-gateway checks that code's license before the copy.

The same server match also answers `ping`, `resources/subscribe`, `resources/unsubscribe`, `tasks/list`, `tasks/result`, and `tasks/cancel`. The 2026-07-28 changelog, read 2026-10-09, removes `ping` from the core protocol, replaces the subscribe pair with `subscriptions/listen`, and moves tasks into an extension that drops `tasks/list`. Those facts constrain a modern request. They are not acceptance items below. Legacy behavior after `initialize` is a separate path.

The 2026-07-28 specification, read 2026-10-09, states these as requirements of a server that implements that revision. They are in scope as obligations. This document does not choose the code that meets them.

- Every modern request carries `io.modelcontextprotocol/protocolVersion` and `io.modelcontextprotocol/clientCapabilities` in `_meta`. The [basic page](https://modelcontextprotocol.io/specification/2026-07-28/basic/index), read 2026-10-09, marks both required. A request missing either is JSON-RPC `-32602`. On HTTP the status is 400. Observed by items 4 and 3.
- The server uses those fields and does not rely on a prior request. A modern `tools/list`, `server/discover`, `tools/call`, or unsupported-version request does not require `initialize`. Observed by items 4, 5, 6, and 7.
- On HTTP the same version is carried in `MCP-Protocol-Version`, and the two must match. The [Streamable HTTP page](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http) requires `Mcp-Method` on every POST. It requires `Mcp-Name` for `tools/call`, `resources/read`, and `prompts/get`, not for `tools/list` or `server/discover`. Observed by items 4, 5, and 7.
- HTTP header validation fails on a missing required header, a header that does not match the body, or a header value with invalid characters. Each of those is HTTP 400 with JSON-RPC code `-32020` (`HeaderMismatch`). Observed by item 4.
- A requested version the server does not implement gets `UnsupportedProtocolVersionError`, code `-32022`, a `supported` list, and the requested version. On HTTP the status is 400. Observed by item 6.
- A modern request for a method the server does not implement returns JSON-RPC `-32601`. On HTTP the status is 404. No acceptance item. This is the fallback of the third open question.
- If a request needs a capability the client did not declare, the server returns `-32021` and, on HTTP, status 400. No acceptance item. The second open question decides whether any modern call needs one.
- Every modern result carries `resultType`. The changelog's values are `complete` and `input_required`. A result from an earlier-protocol server may omit the field. Clients treat that omission as `complete`. The changelog also requires `ttlMs` and `cacheScope` on `tools/list`, `prompts/list`, `resources/list`, `resources/read`, and `resources/templates/list`. It does not name `server/discover` in that list. Observed for `tools/list` by item 4. The other four methods have no acceptance item.
- The changelog replaces a server-sent `elicitation/create`, `sampling/createMessage`, or `roots/list` with an `InputRequiredResult` (`resultType` `input_required`). Sampling is deprecated for new implementations and remains specified during that window. No acceptance item. The second open question decides it.
- Servers implement `server/discover`. Clients may call it and are not required to. On stdio it is the backward-compatibility probe. Observed by item 5.
- A modern success result that does not validate against the schema published for revision 2026-07-28 is not a success. The request validates against the published request schema for that method. The schema page is [2026-07-28 schema](https://modelcontextprotocol.io/specification/2026-07-28/schema). Naming that check does not choose a parser. Observed by items 4, 5, and 7.
- A server may implement both the modern per-request behavior and the legacy `initialize` behavior. AC1 asks for that pair: 2026-07-28 served, with a 2025-11-25 fallback. Observed by items 1 and 4 through 7.

## Acceptance signal

Baseline on `2ba9d37`, separate from the pass condition:

- `MIK-7617.MCP.1` fails. Both client strings receive `protocolVersion` `2025-11-05`.
- `MIK-7617.MCP.2` already holds. After `notifications/initialized`, `tools/list` succeeds and does not read the revision. It is a non-regression. It does not show that a modern client is served.
- `MIK-7617.MCP.3` is unmet. No test asserts the checks below.
- `MIK-7617.MCP.4` fails. A `tools/list` before the phase is `Running` returns "Server not yet initialized", and nothing reads a per-request version.
- `MIK-7617.MCP.5` fails. Before the phase is `Running`, `server/discover` returns "Server not yet initialized". Once the phase is `Running`, it returns method-not-found. Neither result lists supported versions.
- `MIK-7617.MCP.6` fails. Nothing returns code `-32022`.
- `MIK-7617.MCP.7` fails. A `tools/call` before the phase is `Running` returns "Server not yet initialized".

On the later implementation revision, not on this documentation commit, all seven hold. `MIK-7617.MCP.2` already holds on `2ba9d37`.

1. `MIK-7617.MCP.1` An `initialize` whose client `protocolVersion` is `2026-07-28`, and an `initialize` whose client `protocolVersion` is `2025-11-25`, each return a JSON-RPC success whose result `protocolVersion` is `2025-11-25`. That field is not `2025-11-05`. Stdio and HTTP each show this result. These are legacy results. They do not require `resultType`.
2. `MIK-7617.MCP.2` Once the phase is `Running`, `tools/list` still returns a JSON-RPC success for both clients in item 1. This item does not require `resultType`.
3. `MIK-7617.MCP.3` The CI `test` job runs a test for item 1, for both client strings, and a test for each of items 4, 5, 6, and 7, including item 4's header-mismatch case and its missing-required-`_meta` case. Each of those tests fails on `2ba9d37` and passes on the later implementation revision. The item 1 test is about the initialize result's `protocolVersion`. `src/mcp/security.rs` asserts a timestamp that contains the characters `2025-11-05`. That assertion is not item 3, and item 3 does not require it to change.
4. `MIK-7617.MCP.4` A modern `tools/list` returns a JSON-RPC success on stdio and on HTTP. The request carries `2026-07-28` in `_meta["io.modelcontextprotocol/protocolVersion"]` and carries `_meta["io.modelcontextprotocol/clientCapabilities"]`. It does not require a prior `initialize`. The HTTP request also carries `MCP-Protocol-Version: 2026-07-28` and `Mcp-Method: tools/list`. The request validates against the published request schema for that method. The result validates against the schema published for revision 2026-07-28 for that method. A request that omits either required `_meta` field is not this success. The basic page requires that omission to be JSON-RPC `-32602`, and on HTTP status 400. A request whose header and `_meta` version disagree is not this success. The transport page requires that disagreement to be HTTP 400 with JSON-RPC code `-32020`. This item records those requirements.
5. `MIK-7617.MCP.5` A modern `server/discover` uses the same two `_meta` fields and, on HTTP, `MCP-Protocol-Version` plus `Mcp-Method: server/discover`. It does not require a prior `initialize`. It returns a JSON-RPC success on stdio and on HTTP. The request and the result validate against the schemas published for revision 2026-07-28 for that method. The supported versions in that result include `2026-07-28` and `2025-11-25`.
6. `MIK-7617.MCP.6` A modern request whose requested version is neither `2026-07-28` nor `2025-11-25` returns `UnsupportedProtocolVersionError`. It does not require a prior `initialize`. The request carries the same two required `_meta` fields. The code is `-32022`. The supported list includes `2026-07-28` and `2025-11-25`. The error names the requested version. Stdio shows the JSON-RPC error. HTTP shows that error with status 400. This item is not an `initialize`. The open question about other `initialize` strings stays on the legacy path.
7. `MIK-7617.MCP.7` A modern `tools/call` is not answered with "Server not yet initialized". On stdio and on HTTP, one tool the server already lists, whose handler returns without asking the client, returns a JSON-RPC success. The request carries the same two required `_meta` fields, and on HTTP it carries `MCP-Protocol-Version`, `Mcp-Method: tools/call`, and `Mcp-Name` set to that tool's name. It does not require a prior `initialize`. The request and the result validate against the schemas published for revision 2026-07-28 for that method. A tool that needs a live target may still return its existing tool-level error. That error is not the phase rejection.

When item 1 changes the advertised string, `read_system_status`, `MCP_SERVER_DESIGN.md` line 4, and the assertion at `src/mcp/transport.rs` line 516 no longer agree with that string. The same change updates all three so none of them still states that the initialize result is `2025-11-05`.

## Open questions

Each one blocks a solution that depends on it. None is answered by the acceptance items.

| question | owner | what resolves it | when | fallback if unanswered |
|---|---|---|---|---|
| What does `initialize` return for a client string other than `2026-07-28` and `2025-11-25`, including `2025-06-18`, `2025-03-26`, `2024-11-05`, `2025-11-05`, and an unknown string? Design §3.1 describes an echo the code does not perform. | operator | the operator's answer | before the solution review | `initialize` still returns `2025-11-25` for every client string that deserializes. The echo in §3.1 is not selected |
| On a modern request, may the server return `resultType` `input_required`? The 2026-07-28 changelog replaced a server-sent `elicitation/create`, `sampling/createMessage`, or `roots/list` with that result. The nab rule, assumed when the scope was approved, says a call that would ask the client returns an error and does not ask. That rule was not re-asked for axterminator. No tool handler sends either request today. | operator | the operator's answer | before the solution review | the solution adds no new wait and invents no refusal code |
| On a modern request, what happens to `ping`, `resources/subscribe`, `resources/unsubscribe`, and the `tasks/*` methods this server answers today? The changelog removes or relocates them. | operator | the operator's answer | before the solution review | the solution does not add `subscriptions/listen` or the tasks extension. A modern request for a method this server does not implement returns JSON-RPC `-32601`. On HTTP the status is 404 |

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
