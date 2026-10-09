# Test plan — MIK-7617

Status: third statement, awaiting test-plan review. The second statement is `e310943`. Both reviews of it were recorded with process status ok and required repairs. This statement closes those NOW findings inside the plan. It does not edit the approved design. This document specifies tests. It contains no implementation. The approved design is `docs/design/mcp-2026-07-28-design.md` at `cd46747af9b37b089dd6cbaf15e0f91b1f3ed73d`. `change_id` `b150926a565243c93cc3aa52a9657f54f023fca694002298cce42d7f29ba2d78` is that file's sha256 and is not recomputed. The ratified problem is `docs/design/mcp-2026-07-28-problem.md` at `d177a8ec18384aee0a3d7fe06c078865aa3c243d`. Acceptance text is unchanged.

## What this plan is for

Prove each acceptance item in the existing CI job `test`, on the assertions this plan names, using helpers that already exist on parent `2ba9d37`.

## What is out

- A schema file, a parser, and a fetch of the published schema. Question (e) stays deferred. No case compares a file.
- Production behavior. The failing-test commit adds tests, changes one existing assertion literal, and adds a `cfg(test)` app-list stub that `handle_list_apps` consults. When the stub is unset, the call is `list_running_apps`, which is today's behavior. The red baseline for the four cases that set the stub is that commit, not a checkout of `2ba9d37` by itself. `2ba9d37` does not contain the new tests. Those tests use types that exist there. They fail on the assertion this plan names. The stub seam is the only production edit those cases need, and an unset stub does not change behavior.
- A new CI job, a spawned process, and any test whose name contains `live_`.
- Publishing, tagging, and the plugin and directory tickets.
- Rewriting design section 3.1, `MCP_SERVER_DESIGN.md` lines 3117 and 3119, or `src/mcp/security.rs`.
- The 2025-11-25 rule that a legacy HTTP request with no protocol-version meta key and an unsupported `MCP-Protocol-Version` returns 400. The design leaves that residual unimplemented. No case expects that 400.

## Levels and types

Every row is an integration test. It drives `Server::handle` (the function `run_stdio` calls) or `post_mcp`. It is not a pure unit test. Types are functional (a success shape), negative (a pinned error), or regression (a rule that is not its own acceptance item). The suite runs in `.github/workflows/ci.yml` job `test`: `cargo test --all-features -- --skip live_`. HTTP rows live in the `http-transport` test module. That job's `--all-features` enables the module. A run without the feature does not compile those rows. `ax_list_apps` is in the default tool set.

Stdio cases live in `src/mcp/server_tests.rs` and call `Server::handle`. HTTP cases live in the `transport.rs` test module and call `post_mcp`. The HTTP helper is test-only: `post_json_with_headers(state, headers, body) -> (StatusCode, Vec<u8>, Value)`. Empty body bytes stay empty. `Value` is JSON null only when those bytes are empty. A body of the four characters `null` is not empty. The helper is the existing `post_json` with a `HeaderMap` argument and the raw bytes kept. It uses localhost-only auth, peer `127.0.0.1`, and no `Authorization` header. No new production type is added.

## Readings locked here

These are readings of the approved design. They are not a new design statement. The design text stays as the reviewers approved it. An implementation that picks the other reading fails the case named here.

1. The meta version's JSON type is checked before the header is compared to it. A non-string is `-32602` on stdio and on HTTP, not `-32020`. Case R1.
2. The allowed-byte check applies to every header value that reaches `post_mcp`. That is the approved design sentence. This plan does not narrow it. A byte outside `0x09`, `0x20`, and `0x21`–`0x7E` on any such header is `-32020`. R4 is that case. R3 still locks the version header: the header and the meta version are the same forbidden string, so agreement passes and only the byte rule returns `-32020` rather than `-32022`.
3. The status table applies to errors from the modern checks and to the partial-meta `-32602`. That is the sentence after the table in the approved design. The table's heading says the mapping is for a modern request. This plan follows the sentence after the table and does not edit the design. `-32020`, `-32022`, and that `-32602` are HTTP 400. `-32601` from a modern unknown method is HTTP 404. A handler error on a modern request keeps HTTP 200. Case R7 locks that. A legacy request stays HTTP 200. Cases N7 and N8 lock that. A code that is not in the table, including the phase-gate `-32600`, stays HTTP 200. Case N4 locks that.
4. A message with no `id` is still passed to `handle`. HTTP responds `204` with an empty body. No JSON-RPC error body is built. Case N1.
5. The `Mcp-Name` sentinel is decoded with the `base64` crate already in `Cargo.toml`. The suite cannot tell that crate from another decoder that accepts and rejects the same inputs. That choice is review-carried. See surviving mutants.

## Literals

Assertions compare against these literals. They do not compare against a constant defined in the module under test.

- Advertised initialize version: `2025-11-25`
- Modern version: `2026-07-28`
- Also accepted as a modern request version: `2025-11-25`
- Unsupported version: `1900-01-01`
- Version list, in order, length 2: `2026-07-28`, then `2025-11-25`
- `resultType`: `complete`
- `ttlMs`: JSON number `0`. The assertion is equality with `0`, which refuses `1` and every other number.
- `cacheScope`: `public`
- Codes: `-32020`, `-32022`, `-32602`, `-32601`, `-32600`
- Tool name: `ax_list_apps`
- Valid `Mcp-Name` sentinel: `=?base64?YXhfbGlzdF9hcHBz?=` (decodes to `ax_list_apps`)
- Sentinel that decodes to a different name: `=?base64?b3RoZXI=?=` (decodes to `other`)
- Sentinel that does not decode: `=?base64?*?=`

Meta keys, exact:

- `io.modelcontextprotocol/protocolVersion`
- `io.modelcontextprotocol/clientCapabilities`

HTTP success fixtures spell the header names in lowercase (`mcp-protocol-version`, `mcp-method`, `mcp-name`). `HeaderMap` lookup is case-insensitive. A case-sensitive lookup misses them and the success assertion fails.

## Shared fixtures

Modern meta object, unless a case says otherwise:

```json
{"io.modelcontextprotocol/protocolVersion":"<version>","io.modelcontextprotocol/clientCapabilities":{}}
```

`tools/list` and `server/discover` params are `{ "_meta": <that object> }`. `tools/call` params add `"name":"ax_list_apps"` and `"arguments":{}`.

A cold server is `Server::new()` or a fresh `AppState`. No initialize has run.

Every `initialize` in this plan sends `clientInfo` `{"name":"test","version":"1"}` plus `capabilities`. `InitializeParams` requires `clientInfo` (`src/mcp/protocol.rs` lines 124–126). Omitting it returns `-32602` from `handle_initialize` and the version assertion never runs.

A primed server has completed `initialize` and `notifications/initialized`. The initialize client string is `2025-11-05` unless the case names another. Its capabilities object is `{ "sampling": {} }`. The test asserts that this initialize has no `error` before it sends the request under test. Where the request under test is itself a `tools/list`, the test first sends a legacy `tools/list` with no `_meta` and asserts that call has no `error`, which shows `phase` is `Running`. The modern request is a later call on that same server. These cases do not assert the initialize version. Item 1 owns that assertion.

Parent behavior today: production code on this branch is still the parent. `_meta` is ignored. Initialize returns `protocolVersion` `2025-11-05`. Before `Running`, `tools/list`, `tools/call`, and `server/discover` return `-32600` and the message `Server not yet initialized`. After `Running`, `tools/list` and `tools/call` succeed with today's shapes, and `server/discover` returns `-32601`. HTTP status is 200 for those JSON-RPC errors and 204 for a notification.

## Coverage

| id | criterion | case | level | type | parent `2ba9d37` |
|---|---|---|---|---|---|
| MIK-7617.MCP.1 | initialize client `2026-07-28` returns `2025-11-25` | S1, H1 | integration | functional | red: result is `2025-11-05` |
| MIK-7617.MCP.1 | initialize client `2025-11-25` returns `2025-11-25` | S2, H2 | integration | functional | red: result is `2025-11-05` |
| MIK-7617.MCP.1 | client `2025-11-05` still returns `2025-11-25`; no `resultType` | S3, H3, C3 | integration | functional | red: result is `2025-11-05` |
| MIK-7617.MCP.1 | initialize stays legacy even with `_meta` | S4 | integration | regression | red: result is `2025-11-05` |
| MIK-7617.MCP.2 | legacy `tools/list` after each item-1 initialize, no protocol-version meta key | S5, S5b, H5, H5b | integration | regression | green. Header is ignored |
| MIK-7617.MCP.3 | the red rows are in job `test` and fail on the named assertion | the red rows | integration | — | this row is the suite, not a separate fixture |
| MIK-7617.MCP.4 | modern `tools/list` success, cold | S6, H6 | integration | functional | red: `-32600`, no `resultType` |
| MIK-7617.MCP.4 | same success after a primed server | H7 | integration | functional | red: success, `resultType` absent |
| MIK-7617.MCP.4 | `clientCapabilities` key absent | S8, H8 | integration | negative | red: not `-32602` (cold `-32600`, primed success) |
| MIK-7617.MCP.4 | header/meta version disagree | H9 | integration | negative | red: not `-32020` |
| MIK-7617.MCP.5 | modern `server/discover`, cold and primed | S10, H10, H11 | integration | functional | red: cold `-32600`, primed `-32601`, fields absent |
| MIK-7617.MCP.6 | unsupported version `1900-01-01`, cold and primed | S12, H12, H13 | integration | negative | red: not `-32022` |
| MIK-7617.MCP.7 | modern `tools/call` `ax_list_apps`, cold and primed | S14, H14, H15 | integration | functional | red: cold `-32600`; primed success without `resultType` |
| design check 2 | request version `2025-11-25` is served modern | S16, H16 | integration | functional | red: `-32600`, no `resultType` |
| design check 2 | that call does not open a legacy session | S17 | integration | regression | green. Follow-up is already `-32600` |
| design check 1 | a missing required MCP header is `-32020` | R9, R10 | integration | negative | red: cold parent is `-32600` at 200 |
| design check 1 | `Mcp-Method` disagrees with the JSON-RPC method | R11 | integration | negative | red: cold parent is `-32600` at 200 |
| design check 4 | modern unknown method on stdio is `-32601` | S18 | integration | negative | red: cold parent is `-32600` |
| status table | a legacy handler error stays HTTP 200 | N7, N8 | integration | regression | green |
| exempt methods | modern meta and no MCP headers stay on today's handlers | N9, N10, N11, N12, N13 | integration | regression | green |
| design check 1 | forbidden bytes on an agreeing version are `-32020` | R3 | integration | negative | red: cold parent is `-32600` at 200 |
| design check 1 | a forbidden byte on any header that reaches `post_mcp` is `-32020` | R4 | integration | negative | red: cold parent is `-32600` at 200 |
| design check 1 | `MCP-Protocol-Version` absent | R12 | integration | negative | red: cold parent is `-32600` at 200 |
| design check 1 | plain `Mcp-Name` disagrees with `params.name` | R5d | integration | negative | red: cold parent is `-32600` at 200 |
| design check 2 | a non-string meta version is `-32602` on both transports | R1 | integration | negative | red: cold parent is `-32600` |
| design check 2 | capabilities key without the protocol-version key is `-32602` | R2 | integration | negative | red: primed parent succeeds |
| design check 3 | a present null `clientCapabilities` is not an omission | R8 | integration | functional | red: cold parent is `-32600` |
| design check 1 | `Mcp-Name` sentinel decode | R5a, R5b, R5c | integration | functional | red: R5a like H14; R5b and R5c are not `-32020` |
| status table | a handler error on a modern request stays HTTP 200 | R7 | integration | regression | green |
| notification | a message with no `id` gets 204 and an empty body | N1, N2 | integration | regression | green |
| phase gate | an implemented method that is not in the served set keeps today's result | N4, N5 | integration | regression | green |
| progress token | `_meta` with only `progressToken` stays legacy | N14 | integration | regression | green |
| collateral | system-status `protocol_version` | C1 | integration | functional | red: text contains `2025-11-05` |
| collateral | design doc line 4 | C2 | integration | functional | red: line says `2025-11-05` |
| — | no schema file | none | — | — | question (e) is deferred. Named fields are the proof |

MCP.3 has no fixture of its own. It is satisfied when the red rows compile on `2ba9d37`, fail there on the assertion this plan names, and pass after implementation. The security.rs timestamp is not MCP.3.

## Assertions that define a red row

A red row's parent failure is the assertion in this section, not a panic and not a missing import.

S1, S2, S3, H1, H2, H3, S4, C3. `error` is absent. `result.protocolVersion` equals `2025-11-25`. `result` has no `resultType` key. Parent returns `2025-11-05`, so the version equality fails. C3 is the existing assertion at `src/mcp/transport.rs` line 516, changed to the literal `2025-11-25`. The handler still returns `2025-11-05` until implementation.

S6, H6, S10, H10, S14, H14, S16, H16. `error` is absent. `result.resultType` equals `complete`. Parent returns `-32600` and no result, so the `resultType` equality fails.

H7. Same three result fields as H6, and `result.tools` deep-equals H6's `result.tools`. Parent succeeds and omits `resultType`, so the equality fails. The tools-array equality is not the parent failure. It fails later if a primed server's sampling flag changes the list.

H11. Same discover fields as S10. Parent returns `-32601`. The `resultType` equality fails.

H15. Same call fields as S14, with the test stub set. Parent succeeds and omits `resultType`. That equality is asserted first and is the parent failure. Both calls then require `content[0].text` equal to `{"apps":["FixedApp"]}`.

S8, H8. `error.code` equals `-32602`. It does not equal `-32020`. H8 status is 400. Cold parent returns `-32600`. Primed parent returns success. H8 is the primed server, so the parent failure is the missing `-32602`, not the phase error.

H9. `error.code` equals `-32020`. It does not equal `-32022`. Status is 400. Parent cold is `-32600` at status 200. This case is HTTP only.

S12, H12, H13. `error.code` equals `-32022`. It does not equal `-32020` or `-32601`. `error.data.requested` equals `1900-01-01`. `error.data.supported` is the version list. H12 and H13 status is 400. H13 is primed: parent succeeds, so the code equality fails. H12 parent is `-32600`.

## Per-case fixtures

One defect each. The defect is the only input that the assertion is there to refuse.

### Item 1

S1, H1. `initialize`. Client `protocolVersion` `2026-07-28`. Capabilities `{}`. `clientInfo` as in the shared fixture. No `_meta`. No MCP headers.

S2, H2. Same, client `protocolVersion` `2025-11-25`.

S3, H3. Same, client `protocolVersion` `2025-11-05`.

S4. Client `protocolVersion` `2026-07-28`, and `_meta` also carries protocol version `2026-07-28` plus `clientCapabilities` `{}`. Still a legacy initialize result. Exempt methods ignore `_meta`.

### Item 2

S5. After S2's initialize shape and `notifications/initialized`, `tools/list` with params omitted. `error` absent. `result` key set is exactly `tools`. `tools` is an array. Status is not applicable.

S5b. The same `tools/list` after S1's initialize, whose client string is `2026-07-28`. Same assertions.

H5. Same sequence as S5 on one `AppState`. The `tools/list` request sends `mcp-protocol-version: 2025-11-25` and no protocol-version meta key. Status 200. `error` absent. Key set exactly `tools`. Code is not `-32020`.

H5b. The same HTTP `tools/list` after H1's initialize. Same assertions, including the header.

This row is green on the parent. It is not an MCP.3 failing-on-parent proof. The header is in the fixture so the row fails if the header alone selects the modern path: a modern success would add `resultType`, and the exact key set would fail.

### Item 4

S6, H6. Cold. Meta version `2026-07-28`, `clientCapabilities` `{}`. Method `tools/list`. H6 headers: `mcp-protocol-version: 2026-07-28`, `mcp-method: tools/list`. No `mcp-name`.

Success assertions, both: `error` absent. `resultType` `complete`. `ttlMs` equals `0` and is a number. `cacheScope` `public`. `tools` is an array and contains an element whose `name` is `ax_list_apps`. Result key set is exactly `tools`, `resultType`, `ttlMs`, `cacheScope`. H6 status 200.

H7. One test builds a cold `AppState` and a primed `AppState`. Both responses have status 200. It asserts the primed `resultType` equality first. That is the parent failure: the primed parent succeeds and omits `resultType`. Only after that assertion does it compare `result.tools` with the cold result. The comparison is not the parent failure.

S8. Cold stdio. Meta version `2026-07-28`. The `clientCapabilities` key is absent, not null. `error.code` `-32602`, not `-32020`. Parent returns `-32600`. That is the named assertion failing. After implementation, deleting the capabilities check makes this a modern success, because `tools/list` no longer consults `phase`. A check that runs only after the phase gate would leave this case at `-32600`.

H8. Primed. Same omission. Headers agree on `2026-07-28` and `mcp-method: tools/list`. Code `-32602`, not `-32020`. Status 400.

H9. Cold. Meta version `2026-07-28`, `clientCapabilities` `{}`. Header `mcp-protocol-version: 2025-11-25`. `mcp-method: tools/list`. Both versions are supported, so the only defect is the disagreement. Code `-32020`, not `-32022`. Status 400.

### Item 5

S10, H10. Cold. Method `server/discover`. Meta version `2026-07-28`, `clientCapabilities` `{}`. H10 headers: `mcp-protocol-version: 2026-07-28`, `mcp-method: server/discover`.

Assertions: `error` absent. `resultType` `complete`. `ttlMs` equals `0`. `cacheScope` `public`. `supportedVersions` equals the version list (length 2, identity and order). `capabilities` is an object and contains the keys `tools`, `logging`, `resources`, `prompts`, `elicitation`, `tasks`, and `sampling`. `experimental` may be present. The key set of `capabilities` is not pinned. `instructions` may be present. It is not required, and it is not forbidden. The discover object is not an exact key set. H10 status 200.

H11. One test sends H10's request on a cold `AppState`, then the same request on a primed `AppState`. Both responses have status 200. It asserts the primed `resultType` equality first. That is the parent failure: the primed parent returns `-32601`. It then applies the other H10 assertions to both results. The primed result then deep-equals the cold result, including `capabilities` and `instructions` when that key is present. A field that changes because of the earlier initialize fails that equality. The deep-equal is not the parent failure.

### Item 6

S12. Cold stdio. Method `tools/list`. Both meta keys. Version `1900-01-01`. No headers.

H12. Cold HTTP. The same body. Headers `mcp-protocol-version: 1900-01-01` and `mcp-method: tools/list`. Header and meta agree, so the only defect is the unsupported string.

H13. Primed, then the H12 request. Code remains `-32022`. Status 400.

### Item 7

`handle_list_apps` calls `list_running_apps` unless a `cfg(test)` stub is set. The stub is `thread_local` storage of `RefCell<Option<Vec<String>>>`. `None` is today's call. A test that sets it installs a `Drop` guard that writes `None` when the test returns, including on panic. The test harness may reuse a thread, so the guard is required. A test on another thread does not see the value. S14, H14, H15, and R5a set the value to `["FixedApp"]`. Existing tests leave it unset. The red baseline for these four cases is the failing-test commit described under What is out.

S14, H14. Cold. Method `tools/call`. Name `ax_list_apps`. Arguments `{}`. Meta version `2026-07-28`, `clientCapabilities` `{}`. H14 headers: `mcp-protocol-version: 2026-07-28`, `mcp-method: tools/call`, `mcp-name: ax_list_apps` (plain, not the sentinel).

Assertions: `error` absent. `resultType` `complete`. Result key set is exactly `content`, `isError`, `resultType`. No `ttlMs` key. No `cacheScope` key. `isError` is false. `content` is an array of length 1. `content[0].type` equals `text`. `content[0].text` equals the literal `{"apps":["FixedApp"]}`. The response text does not contain `Server not yet initialized`. H14 status 200.

H15. One test sets the same stub, calls H14's request on a cold `AppState`, then on a primed `AppState`. Both responses have status 200. It asserts the primed `resultType` first. That is the parent failure. It then asserts both `content[0].text` values equal `{"apps":["FixedApp"]}`. A handler that drops an app when sampling is set fails the second equality.

### Design checks that acceptance does not number

S16, H16. Cold modern `tools/list` whose meta version is `2025-11-25`, `clientCapabilities` `{}`. H16 header `mcp-protocol-version: 2025-11-25` agrees. H16 status is 200. Same success assertions as S6. This is the case that fails if the allow-list drops `2025-11-25` while still advertising it from discover. Question (d)'s fallback: served under the 2026 result rules, not item 6's error.

S17. A separate test. It sends S16's modern request and does not assert that request's success. It then sends `tools/list` with params omitted and asserts `error.code` equals `-32600` and the message contains `Server not yet initialized`. Parent is green: both calls already return `-32600`. A modern call that moves `phase` to `Running` makes the follow-up succeed, and this test fails.

## Regressions

R1. Cold `tools/list`. `clientCapabilities` is `{}`. The protocol-version meta value is JSON number `1`, not a string. Stdio, and HTTP with `mcp-protocol-version: 2026-07-28` and `mcp-method: tools/list`. Code `-32602`, not `-32020`, not `-32022`. HTTP status 400. Parent cold is `-32600`. If the header is compared before the type check, HTTP returns `-32020` and this case fails.

R2. Primed `tools/list`. `clientCapabilities` is `{}`. The protocol-version key is absent. HTTP headers `mcp-protocol-version: 2026-07-28` and `mcp-method: tools/list`. Also one stdio twin. Code `-32602`, status 400 on HTTP. Not a legacy success. Parent primed succeeds, so the code equality fails. The primed server is the fixture that makes the partial-meta rule the thing that decides. A cold-only fixture would also fail on the phase error.

R3. Cold HTTP `tools/list`. `clientCapabilities` `{}`. `mcp-method: tools/list`. The meta version and `mcp-protocol-version` are the same string: `2026-07-28` plus U+0080. The header bytes are the ASCII of `2026-07-28` followed by `0xC2 0x80`. `HeaderValue::from_bytes` accepts those bytes. The two sides agree, so a missing byte check does not return `-32020` from disagreement. The string is not an allowed version, so a missing byte check falls through to `-32022`. This case asserts `-32020`, not `-32022`, status 400. Parent is `-32600` at 200.

R4. Cold HTTP `tools/list`. Meta and the three MCP headers are the H6 success fixture. An additional header `x-unrelated` has value byte `0x80`. Code `-32020`. Status 400. Parent is `-32600` at 200. That code equality is the named failure. A check limited to the three MCP headers returns the H6 success and fails this case. R3 still separates a forbidden byte from an unsupported version.

R5a. Cold HTTP `tools/call` of `ax_list_apps` with the H14 body, the H14 stub value, and the H14 `Drop` guard. `mcp-name` is `=?base64?YXhfbGlzdF9hcHBz?=`. Same success assertions as H14, including the literal `{"apps":["FixedApp"]}`. Parent fails like H14: no `resultType`.

R5b. Cold HTTP. Same, but `mcp-name` is `=?base64?b3RoZXI=?=`. Code `-32020`, status 400. The only defect is the decoded name.

R5c. Cold HTTP. Same, but `mcp-name` is `=?base64?*?=`. Code `-32020`, status 400. The only defect is a sentinel that does not decode.

R5d. Cold HTTP. The H14 body. `mcp-name` is the plain value `other`. Code `-32020`, status 400. The only defect is the undecoded name. No stub is required, because the case expects an error. Parent is `-32600` at 200.

R6. Cold HTTP. Method `no/such`. Meta version `2026-07-28`, `clientCapabilities` `{}`. Headers agree and `mcp-method: no/such`. Code `-32601`, status 404. Parent cold is `-32600` at 200. A primed twin asserts the same code and status 404. Parent primed already returns `-32601` at 200, so the primed twin's red assertion is the status.

S18. Cold stdio. Method `no/such`. Meta version `2026-07-28`, `clientCapabilities` `{}`. No headers. Code `-32601`, not `-32600`. Parent is `-32600`.

R9. Cold HTTP. The H6 body and `mcp-protocol-version: 2026-07-28`. The `mcp-method` header is absent. Code `-32020`, status 400. Parent is `-32600` at 200.

R12. Cold HTTP. The H6 body and `mcp-method: tools/list`. The `mcp-protocol-version` header is absent. Code `-32020`, status 400. Parent is `-32600` at 200.

R10. Cold HTTP. The H14 body, `mcp-protocol-version: 2026-07-28`, and `mcp-method: tools/call`. The `mcp-name` header is absent. Code `-32020`, status 400. Parent is `-32600` at 200.

R11. Cold HTTP. The H6 body and `mcp-protocol-version: 2026-07-28`. `mcp-method` is `ping`. Code `-32020`, not `-32602`, status 400. The only defect is the method disagreement. Parent is `-32600` at 200.

R7. Primed HTTP. Method `resources/read`. Meta version `2026-07-28`, `clientCapabilities` `{}`. Headers agree and `mcp-method: resources/read`. Params have `_meta` and no `uri`. `error.code` equals `-32602`. Status 200, not 400. Parent already returns `-32602` at 200, because the handler rejects a missing `uri`. This row is green on that parent. It fails if the status table is applied to a handler error. Check 1 does not require `mcp-name` on this method. The approved design applies the table to errors from the modern checks and to the partial-meta `-32602`. This case follows that sentence.

R8. Cold HTTP `tools/list`. Meta version `2026-07-28`. `clientCapabilities` is JSON null, which is present. Headers agree. Same success assertions as H6. Parent fails like H6. Rejecting null as an omission returns `-32602` and fails this case.

## Non-regressions

These pass on the parent. They are here so a later rule cannot turn a current behavior into an error without a failing test. They are not MCP.3's red proof.

N1. HTTP `tools/list` with no `id`. Meta version `2026-07-28`. The `clientCapabilities` key is absent. Headers agree on `2026-07-28` and `mcp-method: tools/list`. Status 204. Raw body bytes are empty. The absent capabilities key is the defect a modern check would answer with `-32602`. Parent already returns 204 for any notification. If the checks run on a message with no `id`, this case returns 400.

N2. HTTP, fresh `AppState`. Send `initialize` with `clientInfo` and client `protocolVersion` `2025-11-25`. Do not send `notifications/initialized` yet. A legacy `tools/list` returns `-32600`. Then send `notifications/initialized` with no `id` and with modern meta version `2026-07-28`. Status 204. Raw body bytes empty. A following legacy `tools/list` with no meta succeeds and its key set is exactly `tools`. `notifications/initialized` advances `phase` only from `Initializing` (`src/mcp/server.rs` lines 240–244). Parent follows that sequence. If the notification is classified and rejected, the follow-up stays `-32600`.

N3. HTTP `ping` before initialize. Both modern meta keys, version `2026-07-28`. No MCP headers. Status 200. `error` absent. `result` is `{}`. No `resultType` key. The missing headers are the defect a non-exempt classification would refuse with `-32020`. Parent serves ping in every phase and ignores meta.

N4. Cold HTTP `resources/list`. Meta version `2026-07-28`, `clientCapabilities` `{}`. Headers agree and `mcp-method: resources/list`. Code `-32600`. Message contains `Server not yet initialized`. Status 200, not 400 and not 404. Parent already does this. A modern unknown-method mapping, or a mapping of every modern error to 400, fails this case.

N5. Primed HTTP `resources/list` with the same modern meta and agreeing headers. Status 200. `error` absent. `result.resources` is an array. `resultType`, `ttlMs`, and `cacheScope` are absent. Parent already returns that list and ignores meta. Adding cache fields to this method fails the case.

N7. Primed HTTP. Method `no/such`. No `_meta`. No MCP headers. Code `-32601`. Status 200, not 404. Parent already returns that error at 200. Applying the modern status table to a legacy request fails this case.

N8. Primed HTTP. Method `tools/call`. Params `{}`. No `_meta`. No MCP headers. Code `-32602`. Status 200, not 400. Parent already rejects the missing name at 200.

N9. Primed HTTP. Method `tasks/list`. Both modern meta keys, version `2026-07-28`. No MCP headers. Status 200. `error` absent. `result.tasks` is an array. No `resultType` key. Code is not `-32020`. Parent serves `tasks/list` after `Running` and ignores meta.

N10. Primed HTTP. Method `resources/subscribe`. Params are `_meta` as in N9 plus `uri` `axterminator://system/status`. No MCP headers. Status 200. `error` absent. Code is not `-32601` and not `-32020`. Parent subscribes that URI.

N11. Primed HTTP. Method `resources/unsubscribe`. Same params and headers as N10. Status 200. `error` absent. Not `-32601`. Not `-32020`.

N12. Primed HTTP. Method `tasks/result`. Params are the N9 meta plus `taskId` `no-such-task`. No MCP headers. Code `-32602`. Status 200, not 400. The handler's missing-task error is the proof the exempt arm ran. Parent already returns `-32602` at 200.

N13. Primed HTTP. Method `tasks/cancel`. Same params and assertions as N12. An unknown `taskId` returns `INVALID_PARAMS` (`src/mcp/server_handlers.rs` lines 475–480). The existing test `tasks_cancel_unknown_task_id_returns_error` already asserts `-32602`. Extra `_meta` deserializes: `TaskCancelParams` has no `deny_unknown_fields`.

N14. Primed HTTP. Method `tools/list`. `_meta` is `{ "progressToken": "1" }` and has neither modern key. Header `mcp-protocol-version: 2025-11-25`. Status 200. `error` absent. Result key set is exactly `tools`. A classifier that treats any `_meta` as modern adds `resultType` and fails the key set. Parent ignores `_meta`.

## Collateral

C1. After `initialize_server`, `resources/read` of `axterminator://system/status`. Parse `result.contents[0].text` as JSON. `protocol_version` equals `2025-11-25`. Parent writes `2025-11-05`.

C2. Read `docs/design/MCP_SERVER_DESIGN.md` from `CARGO_MANIFEST_DIR`. The line that contains `**Protocol**:` contains `MCP 2025-11-25` and does not contain `MCP 2025-11-05`. Parent's protocol line says `MCP 2025-11-05`. Lines 3117 and 3119 are not read. The test does not use a line number.

C3. The existing HTTP reuse test's version assertion becomes the literal `2025-11-25`.

The `protocol.rs` module comment is updated in the implementation because the design says so. No test reads that comment. A comment is not an acceptance observation.

## A1–A9 sweep

A1. `ttlMs` is equality with `0`, not `>= 0`. Codes are equality with one code plus an explicit inequality against the neighboring code the fixture might hit (`-32020` versus `-32022` versus `-32602`).

A2. Modern `tools/list` and modern `tools/call` pin the exact result key set. Discover does not: `instructions` is optional. `capabilities` does not: `experimental` depends on the `watch` feature, and CI passes `--all-features`. Item 2 pins the legacy key set as exactly `tools`.

A3. `supportedVersions` and `data.supported` must equal the two-element list, not merely be a subset of an allowed set. An empty array fails.

A4. The length-2 claim names both strings and their order.

A5. Item 2's header is what a "header means modern" rule would trip. S5b is the same rule after a `2026-07-28` initialize. R2 is primed so the phase gate is not the decider. R1's HTTP header is a supported version so only the non-string type makes `-32602` win over `-32020`. H9 uses two supported versions so the disagreement is the decider. R3's header and meta version are the same forbidden string, so deleting the byte check yields `-32022`. R9, R10, and R12 each omit one required header and nothing else. R4's unrelated byte is the decider for checking every header. R5a sets the stub its literal requires. H11's deep-equal is the decider for discover independence. N1's missing capabilities key is the defect a notification check would refuse. N3 and N9 through N13 omit MCP headers so exemption is the decider. N7 and N8 are legacy errors whose status stays 200. S17's follow-up is the decider for "modern does not open a session."

A6. No assertion uses the current time. The app list in item 7 is the stub literal, not the live process list.



A7. S16 is the case that changes outcome when `2025-11-25` is removed from the allow-list. Discover's list assertion does not prove that a request at that version is accepted.

A8. Expected versions, codes, `complete`, `public`, and `0` are literals in the test. They are not read back from the server's own constant.

A9. Each red fixture has one refused defect. R4's parent failure is `-32600` where `-32020` is required. H13's parent failure is a success where `-32022` is required. R7 is green on the parent. It fails when the status becomes 400.

## Surviving mutants

The suite does not separate these. They are not counted as evidenced rows.

1. Which base64 decoder runs. R5a, R5b, and R5c pin the wire outcomes. Two decoders with those outcomes both pass. Reading carries the crate choice.
2. Schema-file equality. Deferred with question (e). No row claims it.
3. A legacy HTTP request with no protocol-version meta key and an unsupported `MCP-Protocol-Version`. The design does not implement that 400. No row expects it.
4. A header byte the HTTP parser rejects before `post_mcp`. It never becomes a body this suite can assert. R3 does reach `post_mcp`: its version header is the ASCII of `2026-07-28` followed by `0xC2 0x80`, which `HeaderValue::from_bytes` accepts.

The app list is closed. H15 compares both calls to the literal `{"apps":["FixedApp"]}` through the test stub. It is not in the list above.

## Delete-the-rule check

| remove | case that goes green if the fixture is wrong | why it stays red |
|---|---|---|
| modern classification | S6 | expects `resultType` `complete`, parent returns `-32600` |
| header is not a modern signal | H5 | exact key set `{tools}` fails once the header adds modern fields |
| header agreement | H9 | a skipped check returns a modern success, not `-32020` |
| unsupported-version error | S12 | a skipped check returns a modern success, not `-32022` |
| type check before header compare | R1 HTTP | header mismatch returns `-32020` |
| byte check on every header that reaches `post_mcp` | R4 | a three-header check returns success |
| handler error stays HTTP 200 | R7 | applying the table returns 400 |
| discover result ignores the earlier initialize | H11 | the deep-equal fails |
| app-list stub is per thread and cleared on drop | H15 | another test observes `["FixedApp"]` |
| phase gate kept for `resources/list` | N4 | a `-32601` mapping fails the `-32600` assertion |
| `initialize` exempt | S4 | modern checks on initialize reject it or add `resultType` |
| a required MCP header | R9 | a skipped check returns a modern success, not `-32020` |
| notification checks on a message with no `id` | N1 | the checks return 400 |
| `notifications/initialized` still advances phase | N2 | the follow-up stays `-32600` |
| a legacy request stays HTTP 200 | N7 | the status becomes 404 |
| sampling does not change the stubbed app list | H15 | the two texts differ |

## Review questions

1. Every acceptance item has the cases in the coverage table, or the table says why it has none. MCP.3 is the suite. The schema file has none because question (e) is deferred.
2. Each red row names the parent assertion that fails. Green rows say they are green and why they are still in the suite.
3. Canonical DoR for this review is `/Users/mikko/.claude/rules-source/workflows/quality-gates-dor.md` (v6.3.0) plus this plan's A1–A9 sweep. Code, dependency, crypto, and schema-parser gates are N/A: this document specifies tests and does not add a dependency or a parser.
