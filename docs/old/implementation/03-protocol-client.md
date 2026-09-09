# 03: MCP transports and tools-focused client - 2026-09-08

Status: **Done**

Dependencies: 01, 02.

Contract gates: D05; timeouts, supported capabilities, remote-session ownership.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/mcp/ (new or modify when introduced by an earlier phase)
+ tests/mcp_stdio.rs (new or modify when introduced by an earlier phase)
+ tests/mcp_http.rs (new or modify when introduced by an earlier phase)
+ tests/support/ (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Verify the requested protocol revision and authorization/transport specifications against official sources. Record the exact compatibility profile and library gaps in this tracker. Advertise only implemented capabilities; reject unsupported negotiated versions clearly.
2. [x] Implement transport-independent client lifecycle, initialization, request ID correlation, server capability validation, protocol error handling and shutdown. Implement only notifications/server-initiated requests required by the approved profile; explicitly reject unsupported requests.
3. [x] Implement stdio transport with protocol-only stdout, separate bounded/redacted server stderr logs, subprocess exit detection and resource limits. Keep the connection object suitable for supervisor ownership rather than per-CLI process spawning.
4. [x] Implement the approved HTTP transport, required headers/media types, protocol negotiation and any required session/stream behavior. Define reconnect boundaries, redirects and origin checks; do not leak authorization across endpoint changes.
5. [x] Implement paginated tool discovery, tool metadata/schema retrieval and one-shot invocation preserving the result envelope. Distinguish protocol failures from MCP tool-reported failures. Handle discovery invalidation notifications when supported; never treat incomplete discovery as an empty tool set.
6. [x] Add deadlines and cancellation across initialization, discovery and calls. Bound protocol frames/buffers with explicit errors rather than silent truncation. Never automatically replay a tool invocation after uncertain delivery or connection failure.
7. [x] Integrate an authentication-provider interface for phase 05; unauthenticated requests must return actionable authentication-required errors without opening a browser.

## Acceptance criteria

- Local fixtures cover handshake/version mismatch, fragmented messages, pagination, malformed messages, HTTP failures, transport closure and cancellation.
- Concurrent request IDs cannot cross-deliver responses; server logs never corrupt CLI JSON.
- Structured, text and non-text tool results survive serialization without information loss.
- An interrupted potentially side-effecting call is not retried automatically.
- Compatibility profile has been verified and repository validation passes.

## References

v1-scope.md; server-management.md local lifecycle; authentication.md; tool-workflow.md.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: Steps 1-7 and acceptance criteria. The modern discovery probe,
  rather than a legacy initialize handshake, implements this revision's lifecycle.
- In-progress task: None. CLI lifecycle/tool handlers remain deferred to their
  assigned phases; this phase supplies independently usable client APIs.
- Changed paths / commits: `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/error.rs`,
  `src/json.rs`, `src/call.rs`, `src/mcp/`, `tests/mcp_stdio.rs`,
  `tests/mcp_http.rs`, `tests/fixtures.rs`,
  `tests/support/fixtures/{http.rs,process.rs,mcp_stdio.py}`, and implementation
  trackers/contracts. No commits made; README and product documentation unchanged.
- Tests run and results, September 8, 2026:
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: passed, 85 tests.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
  - `cargo test -q --test mcp_stdio --test mcp_http --test fixtures`: passed on
    ten consecutive runs, 31 tests per run.
  - A full-suite run exposed a pre-existing fixture spawn race (`ETXTBSY`). The
    fixture now bounds retries of this pre-exec failure only; production MCP
    requests/process launches do not retry. Subsequent full/repeated runs passed.
- Decisions applied: D05's exact revision, D06's complete result envelope, and
  the existing timeout, local-supervisor and remote-operation ownership contracts.
  No older-version fallback or additional client capability was introduced.
- Blockers / remaining questions: None for phase 03. OAuth discovery/registration,
  issuer/resource validation and secure persistence still require phase 05's
  implementation and D08 audit; this phase does not claim full OAuth support.
- Next action: Begin phase 04 using `Client::stdio` as a supervisor-owned object.
  Recheck current trust/configuration/policy before construction and reuse. Forward
  IPC cancellation into per-operation tokens instead of stopping the shared server.

## Verified compatibility profile

Verified September 8, 2026 against official revision `2026-07-28`. This revision
is available; no replacement revision or user decision was necessary.

- Every request carries the exact revision, empty client capabilities (`{}`),
  and ripmcp identity in the standard `_meta` fields. Connections first send
  `server/discover`, require the requested version in `supportedVersions`, and
  validate advertised tools support. No `initialize` or initialized notification
  is sent. A version mismatch is an explicit protocol failure.
- Implemented RPCs: `server/discover`, paginated `tools/list`, and one-shot
  `tools/call`. Stdio cancellation sends `notifications/cancelled`. Request-related
  notifications are parsed without exposing upstream text. Server-originated
  JSON-RPC requests violate this revision and fail the transport; clients must
  not answer them with JSON-RPC responses.
- Initially no sampling, elicitation, roots, MRTR continuation, subscriptions, tasks,
  resources, prompts, logging-level requests, or caching capabilities are
  advertised. `input_required` is an explicit unsupported result, never a replay.
  The later `call --interactive` addition supports bounded URL elicitation
  continuations only; see commands.md and contracts.md. Noninteractive behavior
  remains unchanged.
  Unknown result kinds are unsupported; malformed envelopes are protocol errors.
- There is no discovery/result cache or subscription stream to invalidate.
  Every call completes fresh paginated discovery before obtaining its schema and
  sending the invocation. Failed pages and repeated cursors fail explicitly.
  Invalid HTTP header annotations exclude the affected tools and remain visible
  in `Discovery.rejected_tools`; cross-server resolution must call
  `Discovery::require_complete`. Qualified lookup need not fail because an
  unrelated tool was excluded.
- Metadata, schemas, result extensions, text/non-text content and arbitrary JSON
  `structuredContent` are retained as JSON values. Arbitrary-precision JSON
  numbers avoid rounding result extensions. This is value preservation, not
  preservation of wire whitespace/object ordering. The shared `json::parse`
  parser rejects duplicate fields and preserves literal keys that resemble
  serde's private number/raw-value tags. Inline call arguments use it too.
  `ToolResult::is_error` reports
  tool failure separately from protocol/transport errors. Schema inspection does
  not resolve remote `$ref` URLs or perform general JSON Schema validation.

### Transport and resource boundaries

- Stdio uses an explicit program/argument vector, explicit environment and
  optional working directory, with piped stdin/stdout/stderr. No shell expansion
  or implicit environment inheritance. Callers prepare the authorized runtime
  environment in later phases. Request IDs are monotonic strings and responses
  correlate through a bounded pending-request registry. Late/duplicate responses
  to retired IDs cannot satisfy a new request; unsolicited future IDs fail.
- Stderr is continuously drained into a fixed-size scratch buffer. Its contents
  are fully redacted: `StderrLog` retains only saturating byte/chunk counters,
  never raw text or secret-dependent partial redactions. Neither transport writes
  diagnostics/results to CLI stdout.
- Closing the client stops outstanding work. Stdio closes stdin, allows 250 ms
  for exit, sends SIGTERM, allows another 250 ms, then kills/reaps the owned child.
  Buffered stdout is drained briefly after subprocess exit. Failed connection
  setup cleans up the newly owned process. Cancelling a reused client's request
  leaves its subprocess alive. Process-group/grandchild ownership is phase 04.
- HTTP uses POST, JSON request bodies, both required Accept media types, exact
  protocol/method headers, and encoded `Mcp-Name`. Tool argument header mirroring
  supports nested property paths, safe integer bounds, omitted/null values, and
  Base64 sentinel escaping. Invalid annotation locations/types/names and
  case-insensitive duplicates are rejected per the binding.
- Both JSON and request-scoped SSE replies are supported, including fragmented
  UTF-8, multiline data, comments, and CR/LF/CRLF framing. There are no protocol
  sessions, GET/DELETE endpoints, `Last-Event-ID`, stream resumption, reconnection,
  or automatic request retries in this profile. Closing the HTTP response stream
  is cancellation; no cancellation POST is sent.
- HTTP/1.1 with verified TLS is used; idle connection pooling, automatic proxy
  discovery, redirects, Referer propagation and request retries are disabled.
  Endpoints require HTTPS except unauthenticated loopback HTTP. URL userinfo and
  fragments are rejected. Configured credential headers and provider credentials
  require HTTPS. Protocol/routing/authorization headers cannot be overridden via
  extra headers. A new endpoint requires a newly authorized client/provider.
- Defaults: 16 MiB per protocol frame/SSE event, 32 MiB cumulative discovery,
  10,000 tools, 1,000 pages, 128 in-flight requests. Positive explicit `Limits`
  can adjust these; invalid limits fail before connection. Queues and cancellation
  writes share bounded request slots. Bounds fail explicitly, not with partial
  success or truncated results. JSON parser nesting is also bounded.
- `Operation` combines the existing monotonic `Deadline` with a cancellation
  token. Connection/provider waits, queueing, all discovery pages and invocation
  consume the same budget. No progress/page resets it. Cleanup can require its
  separate bounded process-exit grace after the operation deadline expires.
  Synchronous OS/JSON work is not preempted; deadline checks after synchronous
  work prevent reporting success past the budget. Install `SignalCancellation` for the
  short-lived CLI's lifetime; supervisor IPC requests need individual tokens.

### Authorization boundary and library audit

`AuthenticationProvider` receives the exact endpoint and existing operation
budget for each request. Its challenge hook receives 401/403 headers, including
`WWW-Authenticate`, bound to that endpoint. Provider methods must remain
noninteractive and return sanitized errors. The client never opens a browser,
follows a challenge URL, retries after a challenge, or puts a token in a URL.
Configured Authorization references belong in the provider, not extra headers.
No-auth defaults produce actionable explicit-login errors on 401/403.

The official authorization profile requires OAuth 2.1, protected-resource and
authorization-server metadata discovery, registration/client identity, PKCE,
issuer validation, resource indicators and protected credential storage. These
remain phase 05 work. Challenges/credentials are not cached under display names
by this client, and SDK authorization support is not assumed to satisfy D08.

Audited Rust SDK tag `rmcp-v3.2.0`, commit
`51ccb42993d6eb5075399672ce7a0c21a0e55eea`:

- `ProtocolVersion::V_2026_07_28` exists, but `LATEST` remains `V_2025_11_25`.
  `ClientLifecycleMode::Discover` offers modern operation; the ordinary serve
  helpers still default to initialization, and `Auto` allows legacy fallback.
- Streamable HTTP includes modern metadata/header mirroring and annotation
  validation, alongside legacy session/recovery paths. This is not a claim that
  its explicit modern mode cannot work; its defaults are not ripmcp's profile.
- `CallToolResult` has fixed result fields without an extension flatten map, and
  its deserializer defaults absent content. Using it as the result envelope
  would lose unknown result fields and relax ripmcp's strict modern validation.

Consequently this phase uses a repository-owned raw-JSON protocol layer rather
than adding rmcp. `Cargo.lock` pins reqwest 0.13.4, Tokio 1.53.1, tokio-util 0.7.19,
base64 0.22.1 and serde_json 1.0.151. HTTP, TLS, async I/O and cancellation use
these libraries; protocol/profile behavior is tested locally. The Python stdio
fixture requires `/usr/bin/python3`; no npx/uvx/Docker installation, public MCP
endpoint, personal configuration or credential store is used by the tests.

### Audit sources

Official pages inspected:

- `https://modelcontextprotocol.io/specification/2026-07-28/server/discover`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http`
- `https://modelcontextprotocol.io/specification/2026-07-28/server/tools`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations`

Pinned primary-source audit anchors:

- `https://github.com/modelcontextprotocol/modelcontextprotocol/blob/aa8ce049f089f92618340190d4ece141f663310d/schema/2026-07-28/schema.ts`
- `https://github.com/modelcontextprotocol/rust-sdk/tree/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src`
  (`model.rs`, `service/client.rs`, `transport/streamable_http_client.rs`,
  `transport/auth.rs`; also `crates/rmcp/Cargo.toml`).
