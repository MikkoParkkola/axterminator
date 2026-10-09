# Solution design — MIK-7617

Status: both design reviews approved commit `cd46747af9b37b089dd6cbaf15e0f91b1f3ed73d` on 2026-10-10. GPT run `ax7617-design2-cd46747`, ledger `2026-10-09T22:39:17Z`. Claude ledger `2026-10-09T22:49:35Z`, model `claude-opus-5`. `change_id` `b150926a565243c93cc3aa52a9657f54f023fca694002298cce42d7f29ba2d78` is the sha256 of this file at that commit and is not recomputed. The test plan has not started. This document chooses the approach. It contains no implementation. The ratified problem is `docs/design/mcp-2026-07-28-problem.md` at commit `d177a8ec18384aee0a3d7fe06c078865aa3c243d`. Two status sentences in that file point here. The acceptance text is unchanged.

The first statement is `56e08df`. Both reviews of it were SHIP-WITH-FIXES. This statement closes four NOW findings. A modern request is recognized only by the `_meta` protocol-version key, so a legacy HTTP client that sends `MCP-Protocol-Version` stays legacy. A non-string version is `-32602`. `clientCapabilities` without that version key is `-32602`. Header values that reach `post_mcp` are checked for the spec's allowed bytes, and a failure is `-32020`.

## What this change is for

A client that sends revision `2026-07-28` gets a successful axterminator response for `tools/list`, `server/discover`, and one `tools/call` that finishes without a server-to-client question. An `initialize` answers `protocolVersion` `2025-11-25`.

## What is out

- MIK-7618, the plugin folder. MIK-7604, the directory submission. MIK-7438.
- A crate publish. A tag move. This document does not push the branch. On 2026-10-10 the operator ordered the change shipped. Shipping means merged to the default branch. That instruction approves a later push, pull request, and merge. It does not approve a crate publish or a tag move.
- Copying trvl, mcp-gateway, or nab's vendored SDK. This crate stays hand-rolled. No `rust-mcp-sdk`.
- Rewriting design §3.1. Editing `MCP_SERVER_DESIGN.md` lines 3117 and 3119. Editing the timestamp assertion in `src/mcp/security.rs`.
- `subscriptions/listen`. The tasks extension. A new wait. `resultType` `input_required`. A new refusal code.
- Resetting `phase`, or cloning the server per request, as the mechanism.
- Requiring a bearer token on the acceptance calls.
- A schema file, a parser, or a fetch of the schema during the test.
- Lifting `resources/list`, `resources/templates/list`, `resources/read`, `prompts/list`, or `prompts/get` off the phase gate. Those methods have no acceptance item. A scope challenge is how one of them enters the served set.

## Decisions already recorded

Asked and answered, or selected by an instruction not to wait. They are not a Linear comment.

- 2026-10-09. The legacy fallback string is `2025-11-25`. The plugin-validate criterion stays on MIK-7618.
- 2026-10-10. The operator said to finish and ship, and not to stop. The five open questions stay unanswered. Their recorded fallbacks are in force. This design uses those fallbacks. It does not invent a different answer.

Vendor seating. `docs/design` process text names gpt-review and grok-review. This author is Grok. The pair for this review is gpt-review and claude-review (`claude-opus-5`, safe-mode). grok-review would be Grok reviewing Grok. The process sentence is not edited by this change.

## Unknowns

| question | state | effect on this design |
|---|---|---|
| What `initialize` returns for a client string other than `2026-07-28` and `2025-11-25` | unanswered; fallback in force | every client string that deserializes gets `protocolVersion` `2025-11-25`. The §3.1 echo is not built |
| Whether a modern result may be `input_required` | unanswered; fallback in force | no new wait, no new refusal code. Success results use `resultType` `complete` |
| Modern behavior of `ping`, `resources/subscribe`, `resources/unsubscribe`, and `tasks/*` | unanswered; fallback in force | those methods keep today's handlers, including today's phase gate. This design does not answer them with `-32601`. It does not add `subscriptions/listen` or the tasks extension |
| A modern request whose `_meta` version is `2025-11-25` | unanswered; fallback in force | the request is supported. It is served under the 2026-07-28 result rules. It does not join a legacy `initialize` session. It is not item 6 |
| How CI obtains the published schema | deferred | no fetch, no committed copy, no parser. Named fields are what the tests assert. The file comparison is not built |

The schema question, deferred:

| field | value |
|---|---|
| owner | the operator |
| what would resolve it | the operator's answer, including whether a copy may be committed, and the schema license |
| when | before any test that compares a schema file is added |
| what if it resolves badly | this change still has no file comparison. The named-field assertions remain the proof |

Nothing in this design depends on a schema file. `DiscoverResult.capabilities` is included because the published schema requires that field (`schema.ts`, `DiscoverResult`, read 2026-10-10). Item 5 does not name it as an assertion. Whether the capability object this server already serializes satisfies the 2026 `ServerCapabilities` schema is part of the deferred file comparison.

## Alternatives rejected

1. Adopt nab's vendored `rust-mcp-sdk` patch, or copy the trvl / mcp-gateway server. Rejected. The problem quarantines that code. This crate has no SDK dependency. The acceptance is a wire result, and the hand-rolled types already produce JSON-RPC results.

2. Make `initialize` echo the client version, as design §3.1 describes, including `2025-06-18` and `2025-03-26`. Rejected. The fallback for the first open question says the echo is not selected. Item 1 requires the result `protocolVersion` to be `2025-11-25` for both named client strings. One builder that echoes would make a `2026-07-28` initialize return `2026-07-28`, which item 1 forbids.

3. Advertise `2026-07-28` from `initialize`. Rejected. Item 1 requires `2025-11-25`. A modern client does not open with `initialize`. The versioning page, read 2026-10-09, says that revision has no negotiation handshake.

4. Reset `phase` after each request, or clone a fresh server for every modern request, as the required mechanism. Rejected. The acceptance is that the modern result does not depend on an earlier `initialize`. The legacy client on the same HTTP handle still needs the phase that `initialize` set. A reset would make the next legacy `tools/list` fail. A clone would make `ping`, subscribe, and `tasks/*` see empty state, and that would decide the third open question. Those methods keep today's handlers on the shared handle.

5. Treat the JSON-RPC success that already follows `notifications/initialized` as items 4 and 7. Rejected. `ToolListResult` is only `tools` (`src/mcp/protocol.rs` lines 327–329). `ToolCallResult` is `content` and `isError` (lines 359–363). Item 4 requires `resultType`, `ttlMs`, and `cacheScope`. Item 7 requires `resultType` and does not require the cache fields. `CallToolResult` extends `Result`, not `CacheableResult` (`schema.ts` line 1819, read 2026-10-10).

6. Require a bearer token on every HTTP acceptance call. Rejected. `AuthConfig::LocalhostOnly` accepts a loopback peer with no `Authorization` header (`src/mcp/auth.rs` lines 186–196). `post_mcp` runs `check_auth` before dispatch (`src/mcp/transport.rs` lines 278–279). Items 4 through 7 are the response after that check. An HTTP 401 is not those items.

7. Answer a modern `ping`, `resources/subscribe`, `resources/unsubscribe`, or `tasks/*` with `-32601`. Rejected. That would decide the third open question. The fallback says the `-32601` rule is not a decision about those methods.

8. Fetch the 2026-07-28 schema in the test job, or commit a copy and choose a parser. Rejected for this change. The fallback says the job does not fetch the schema and no parser is chosen. The file comparison stays an obligation that waits.

9. Serve the modern revision on a second port or a second path. Rejected. The versioning page says a dual-era server may serve both eras on one endpoint. A second route would leave the shared-handle clause of items 4 through 7 untested on the route clients already use.

10. Require a second result type for every modern success. Rejected as a requirement. The wire object is the legacy result plus the named fields. A second type would duplicate the tool-list builder, or it would change the legacy shape, which item 2 does not ask for. The implementation may wrap the legacy value or add the fields after serialization. Either one meets the wire object. The choice is not a second protocol.

## Chosen approach

Classify each JSON-RPC request. `initialize` stays the legacy path and always returns `2025-11-25`. A request that carries the per-request protocol version is modern. Modern `tools/list`, `tools/call`, and `server/discover` run without consulting `phase` or `client_supports_sampling`. Their JSON results gain the fields named below. HTTP status is chosen at `post_mcp` from the JSON-RPC code. Stdio has no status.

Two transports keep two instances of the same server. Stdio still constructs `Server::new()` (`src/mcp/server.rs` line 328) and calls `Server::handle` (line 357). HTTP still stores one `ServerHandle` in the `AppState` mutex (`src/mcp/transport.rs` lines 181–188 and 207) and `post_mcp` still locks it (lines 296–297). This design does not merge those instances.

## Classification

These methods are exempt. They ignore `_meta` and the MCP headers. They use today's match arms, including today's phase gate:

- `initialize`
- `notifications/initialized`
- `ping`
- `resources/subscribe`
- `resources/unsubscribe`
- `tasks/list`
- `tasks/result`
- `tasks/cancel`

`initialize` is exempt so item 1 stays a legacy result even if a caller also sends `_meta`. The four methods from the third open question are exempt so this design does not choose their modern behavior. `notifications/initialized` stays the notification that moves `Initializing` to `Running`.

Every other method is classified from the request. There is one classifier. `post_mcp` and `Server::handle` both use it.

A request is modern only when `params._meta["io.modelcontextprotocol/protocolVersion"]` is present. The header `MCP-Protocol-Version` is not a modern signal. The [2025-11-25 transport page](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), read 2026-10-10, requires that header on every HTTP request after initialize. A legacy client does not send the modern `_meta` keys. Classifying on the header would reject that client.

On a non-exempt method, `params._meta["io.modelcontextprotocol/clientCapabilities"]` without the protocol-version key is not legacy. The response is JSON-RPC `-32602`. On HTTP the status is 400. A request with neither modern key is legacy, including an HTTP request that carries `MCP-Protocol-Version` and no `_meta`. A `_meta` object that carries only a progress token stays legacy. That key is not one of the two modern keys.

If the request is legacy, today's dispatch runs. A legacy `tools/list` still requires `phase == Running`. A legacy `server/discover` is still "Server not yet initialized" before `Running`, and method-not-found after `Running`. Header names are matched case-insensitively. The version strings are exact.

`ToolCallParams` does not declare `_meta` (`src/mcp/protocol.rs` lines 333–337) and does not deny unknown fields. A modern `tools/call` can carry `_meta` beside `name` and `arguments` and still deserialize. The meta keys are read from the raw params value.

## Checks on a modern request

The first failure is the response. Later checks do not run.

1. HTTP only, and only after the request has been classified modern. Stdio skips this step. A message with no `id` is a notification. `post_mcp` does not build a JSON-RPC error for it. `Server::handle` already returns no body for a missing id (`src/mcp/server.rs` lines 165–168).

`MCP-Protocol-Version` and `Mcp-Method` must be present. `tools/call` also requires `Mcp-Name`. Each raw header value that reaches `post_mcp` must be made of bytes `0x09`, `0x20`, and `0x21` through `0x7E`. Any other byte is JSON-RPC `-32020` and HTTP 400. That is the invalid-character rule on the [streamable HTTP page](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http), Value Encoding, read 2026-10-10. A value the HTTP parser rejects before `post_mcp` never becomes a JSON-RPC body.

`Mcp-Name` may be the sentinel `=?base64?` … `?=`. The markers are exact and lowercase. The middle is decoded as base64 before it is compared with `params.name`. A sentinel that does not decode is `-32020`. `MCP-Protocol-Version` and `Mcp-Method` are compared as received. They are not decoded.

The header values must then agree with the body. `MCP-Protocol-Version` equals the meta version string. `Mcp-Method` equals the JSON-RPC method. `Mcp-Name`, after that decode when the sentinel is present, equals `params.name`. A missing header, or a disagreement, is `-32020` and HTTP 400. `post_mcp` returns that error and does not call `handle`. An empty header value is present and does not equal a non-empty body value. Item 4's required mismatch case is the version disagreement, pinned below.

The transport page also requires `Mcp-Name` for `resources/read` and `prompts/get`. Those methods are not in the served set, so this change does not add that check for them.

A legacy HTTP request may carry `MCP-Protocol-Version: 2025-11-25` and no protocol-version meta key. It stays on today's dispatch. This change does not return 400 when that header is unsupported and the protocol-version meta key is absent. The 2025-11-25 page requires that 400. It is not an acceptance item.

2. The meta version must be a JSON string. A value that is not a string is `-32602`, not `-32022`. On HTTP the status is 400. A string other than `2026-07-28` and `2025-11-25` is `UnsupportedProtocolVersionError`. The code is `-32022`. `data.supported` is the one list named under collateral strings. `data.requested` is that string. HTTP status is 400. This is not `-32020` and not `-32601`. The string `2025-11-25` passes. `RpcError` already has a `data` field (`src/mcp/protocol.rs` lines 64–70). `RpcError::new` leaves it empty (lines 81–86). The unsupported-version error sets `data`. No new error type is required.

3. The key `io.modelcontextprotocol/clientCapabilities` must be present on `_meta`. If the key is absent, the code is `-32602` and, on HTTP, the status is 400. That is not `-32020`. A present value of any JSON type, including an empty object and null, is not this failure. This change does not validate the shape of that value. Shape validation is part of the deferred schema comparison.

4. Dispatch. `tools/list`, `tools/call`, and `server/discover` run even when `phase` is not `Running`. They do not read `client_supports_sampling`. Modern `tools/call` uses the same tool dispatch the legacy call uses, including the security gates on that path. The sampling read stays inside the `ax_find_visual` branch. Any other method that today's match already implements (`resources/list`, `resources/templates/list`, `resources/read`, `prompts/list`, `prompts/get`) falls through to today's phase-gated arm. A method today's match does not implement returns `-32601`. On HTTP that status is 404. Before `Running`, today's code returns "Server not yet initialized" for that unknown method (`src/mcp/server.rs` lines 218–223). A modern unknown method returns `-32601` instead. The exempt methods never reach this step.

## Results

Legacy `tools/list` and legacy `tools/call` keep today's JSON shape. Item 2 does not gain `resultType`. The modern path adds fields on the JSON value. The legacy structs stay the legacy wire shape.

| result | fields this change sets |
|---|---|
| modern `tools/list` | today's `tools` array, `resultType` `complete`, `ttlMs` `0`, `cacheScope` `public` |
| modern `server/discover` | `supportedVersions` containing `2026-07-28` and `2025-11-25`, `resultType` `complete`, `ttlMs` `0`, `cacheScope` `public`, `capabilities` equal to the object `initialize` already returns |
| modern `tools/call` | today's `content` and `isError`, plus `resultType` `complete`. No `ttlMs`. No `cacheScope` |
| legacy `initialize` | today's result, with `protocolVersion` `2025-11-25`. No `resultType` requirement |

`ttlMs` `0` is the value the schema defines as immediately stale (`schema.ts`, `CacheableResult.ttlMs`, read 2026-10-10). The tool list is filtered by the security mode (`src/mcp/server_handlers.rs` lines 78–88). This change does not promise a cache window. `cacheScope` `public` matches a payload that does not vary by the caller. Both values are schema-valid (`ttlMs` minimum 0, `cacheScope` `public` or `private`).

`server/discover` is not added to the legacy path. `instructions` on that result is optional in the schema. The acceptance does not require it. The implementation may copy the instructions string `initialize` already returns. Item 5 does not assert it.

The capability object on that result is the object `initialize` already returns. It advertises resources, prompts, and tasks. The modern path does not serve those methods. Trimming the object would decide the third open question, so the object stays. Whether that advertisement is accurate waits on that question.

The proof tool for item 7 is `ax_list_apps`. `handle_list_apps` returns `ToolCallResult::ok` of the running-app list (`src/mcp/tools_gui.rs` lines 592–595). It does not read `client_supports_sampling`. The only reader of that flag on the tool path is `ax_find_visual` (`src/mcp/server_handlers.rs` lines 553–559). `ax_list_apps` is already listed and the existing suite already calls it. The call does not ask the client. A live GUI target is not required.

`handle_tools_call` can write `notifications/resources/updated` when the legacy subscription set is non-empty (`src/mcp/server_handlers.rs` lines 570–576 and 594). That write is outside the JSON-RPC result. Items 4 and 7 assert the result. This design does not clear the subscription set. The third open question owns that state. The proof call is `ax_list_apps`, whose result JSON does not change when the set is non-empty.

## HTTP status

`post_mcp` today returns the JSON body with axum's default status, which is 200 (`src/mcp/transport.rs` lines 318–320). This change maps status for a modern request, and for the partial-meta `-32602`:

| code | HTTP status |
|---|---|
| `-32020` | 400 |
| `-32022` | 400 |
| `-32602` | 400 |
| `-32601` | 404 |

`post_mcp` applies that table to errors from the modern checks and to the partial-meta `-32602`. A legacy handler's error keeps today's status, including a legacy `-32602` and a legacy `-32601`. Auth is unchanged. A failed `check_auth` still returns before classification.

## Shared handle

Items 4, 5, 6, and 7 require the modern outcome after another request on that HTTP server has completed `initialize` and `notifications/initialized`. The modern handlers do not read `phase` or `client_supports_sampling`. An earlier initialize may set both (`src/mcp/server_handlers.rs` lines 55–56). The modern result is built without those fields. A sampling capability declared only by the earlier client does not change `tools/list`, `server/discover`, the unsupported-version error, or the `ax_list_apps` result.

The HTTP test for that clause uses the same `AppState` for both requests, localhost-only auth, peer `127.0.0.1`, and no `Authorization` header. That is the setup `post_mcp_reuses_server_state_across_requests` already uses.

## Collateral strings

When item 1 changes the advertised string, the same change sets these three to `2025-11-25`:

- `build_initialize_result` (`src/mcp/server_handlers.rs` line 738)
- `read_system_status` (`src/mcp/resources_read.rs` line 32)
- `docs/design/MCP_SERVER_DESIGN.md` line 4
- the assertion at `src/mcp/transport.rs` line 516

The client strings sent by `src/mcp/server_tests.rs` stay `2025-11-05`. Those tests send a client string. They do not assert the result string. The fallback accepts that client string and still answers `2025-11-25`.

The initialize result, the system-status field, design line 4, and the transport assertion are one value, `2025-11-25`. `supportedVersions` and `data.supported` are one list: `2026-07-28`, then `2025-11-25`. The implementation keeps that list in one place.

Section 3.1 is not rewritten. Lines 3117 and 3119 are not edited. `src/mcp/security.rs` is not edited. The module comment at `src/mcp/protocol.rs` line 1 says the types are MCP 2025-11-25, wire-compatible with 2025-11-05. That comment is updated so it does not claim the types are only that revision. The comment is not a wire result.

## Tests

The test plan is the next document. This section is the constraint that plan has to meet. It is not the plan.

The existing CI job `test` stays the only protocol job (`.github/workflows/ci.yml`). New tests live in that suite. They do not fetch a schema and they do not skip with `live_`. They call `Server::handle`, which is the function `run_stdio` calls, and they call `post_mcp`. They do not require a spawned process. They use types and helpers that exist on `2ba9d37`, so they compile there. Items 1, 4, 5, 6, and 7 fail on that parent on the named assertion. Item 2 holds there. On the implementation revision all seven hold.

Pinned requests, so each failure has one cause:

- Item 1. Two initializes, client `protocolVersion` `2026-07-28` and `2025-11-25`. Result `protocolVersion` is `2025-11-25`. Stdio handle and HTTP.
- Item 2. After each of those initializes, `notifications/initialized`, then `tools/list` with no protocol-version meta key. On HTTP the request also sends `MCP-Protocol-Version: 2025-11-25`. JSON-RPC success. The code is not `-32020`. `resultType` is not required. This case holds on `2ba9d37`, because that parent ignores the header. It fails if the header alone selects the modern path.
- Item 4 success. Meta version `2026-07-28`, `clientCapabilities` present, no prior initialize. Stdio sends no headers. HTTP also sends `MCP-Protocol-Version: 2026-07-28` and `Mcp-Method: tools/list`. Result has `resultType`, `ttlMs`, and `cacheScope`.
- Item 4 omission. Same versions, both sides `2026-07-28`, `Mcp-Method: tools/list`, `clientCapabilities` key absent. Not null. Code `-32602`, HTTP 400. Stdio is the same omission with no headers.
- Item 4 mismatch. Meta `2026-07-28`, header `2025-11-25`, `Mcp-Method: tools/list`, `clientCapabilities` present. Both versions are supported, so the failure is the disagreement. Code `-32020`, HTTP 400. Stdio has no header, so this case is HTTP only.
- Item 5. Stdio: the two meta fields and no headers. HTTP: those fields, `MCP-Protocol-Version`, and `Mcp-Method: server/discover`. Result has `resultType`, `ttlMs`, `cacheScope`, and `supportedVersions` containing both revisions.
- Item 6. `tools/list`, both meta fields, version `1900-01-01` on the meta value. HTTP also sets `MCP-Protocol-Version` to `1900-01-01` and `Mcp-Method: tools/list`. Stdio sends no headers. Code `-32022`, `data.supported` contains both revisions, `data.requested` is `1900-01-01`. HTTP 400.
- Item 7. Stdio: `tools/call` of `ax_list_apps` with the two meta fields and no headers. HTTP: those fields, `MCP-Protocol-Version`, `Mcp-Method: tools/call`, and `Mcp-Name: ax_list_apps`. JSON-RPC success, `resultType` present, body is not "Server not yet initialized".
- Shared handle. For items 4, 5, 6, and 7, repeat the HTTP case on an `AppState` that has already completed `initialize` and `notifications/initialized`, with the earlier client declaring sampling. The modern outcome still holds.

The full schema-file comparison is not one of these tests.

## How the acceptance is met

1. `MIK-7617.MCP.1` Exempt `initialize`. `build_initialize_result` returns `2025-11-25` for every client string that deserializes, including the two named strings and `2025-11-05`. Stdio and HTTP both use that result. No `resultType` is required.
2. `MIK-7617.MCP.2` A `tools/list` with no protocol-version meta key, after `notifications/initialized`, still uses today's handler. On HTTP that request may carry `MCP-Protocol-Version: 2025-11-25`. The legacy result stays a JSON-RPC success without a required `resultType`. The code is not `-32020`.
3. `MIK-7617.MCP.3` The tests in the previous section run in the existing `test` job, compile on `2ba9d37`, fail there on the named assertion, and pass on the implementation. No schema file.
4. `MIK-7617.MCP.4` A modern `tools/list` skips the phase gate and adds `resultType`, `ttlMs`, and `cacheScope`. The omission case fails check 3. The mismatch case fails check 1. The shared-handle case uses the same handler, which does not read the earlier initialize.
5. `MIK-7617.MCP.5` A modern `server/discover` skips the phase gate and returns the fields in the results table, including both versions in `supportedVersions`.
6. `MIK-7617.MCP.6` Check 1 passes because the header and the body agree. Check 2 returns `-32022` with `data.supported` and `data.requested`. A meta version of `2025-11-25` does not enter this item.
7. `MIK-7617.MCP.7` A modern `tools/call` skips the phase gate. `ax_list_apps` returns a JSON-RPC success. The result gains `resultType` and does not gain the cache fields. The phase error is not the response. A tool that needs a live target may still return its existing tool-level error. That error is not this proof.

## Constraint check

- Both required `_meta` fields are enforced for a modern request. The missing-field code is `-32602`.
- The three served methods do not read `phase` or `client_supports_sampling`, including after a legacy initialize on the shared HTTP handle.
- HTTP carries `MCP-Protocol-Version` and `Mcp-Method`. `tools/call` also carries `Mcp-Name`. Disagreement is `-32020` before the version check.
- An unsupported version on a non-exempt modern request is `-32022` with the data object. HTTP 400.
- A modern request for a method this server does not implement is `-32601`. HTTP 404. The exempt methods are not given that error.
- `-32021` is not emitted. No modern call in this design requires a capability beyond the presence check in step 3. The second open question still decides whether any call needs a declared capability.
- Modern successes use `resultType` `complete`. `tools/list` and `server/discover` include `ttlMs` and `cacheScope`. `prompts/list`, `resources/list`, `resources/read`, and `resources/templates/list` are not in the served set, so they do not gain those fields here.
- No `input_required` result is produced.
- `server/discover` is implemented for a modern request. It is the stdio backward-compatibility probe for that request.
- The schema-file comparison waits. The named fields are present so that comparison has a result to read once the operator answers.
- Both eras stay on one endpoint. `initialize` remains available and answers `2025-11-25`.
