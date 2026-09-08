# 03: MCP transports and tools-focused client - 2026-09-08

Status: **Not started**

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

1. [ ] Verify the requested protocol revision and authorization/transport specifications against official sources. Record the exact compatibility profile and library gaps in this tracker. Advertise only implemented capabilities; reject unsupported negotiated versions clearly.
2. [ ] Implement transport-independent client lifecycle, initialization, request ID correlation, server capability validation, protocol error handling and shutdown. Implement only notifications/server-initiated requests required by the approved profile; explicitly reject unsupported requests.
3. [ ] Implement stdio transport with protocol-only stdout, separate bounded/redacted server stderr logs, subprocess exit detection and resource limits. Keep the connection object suitable for supervisor ownership rather than per-CLI process spawning.
4. [ ] Implement the approved HTTP transport, required headers/media types, protocol negotiation and any required session/stream behavior. Define reconnect boundaries, redirects and origin checks; do not leak authorization across endpoint changes.
5. [ ] Implement paginated tool discovery, tool metadata/schema retrieval and one-shot invocation preserving the result envelope. Distinguish protocol failures from MCP tool-reported failures. Handle discovery invalidation notifications when supported; never treat incomplete discovery as an empty tool set.
6. [ ] Add deadlines and cancellation across initialization, discovery and calls. Bound protocol frames/buffers with explicit errors rather than silent truncation. Never automatically replay a tool invocation after uncertain delivery or connection failure.
7. [ ] Integrate an authentication-provider interface for phase 05; unauthenticated requests must return actionable authentication-required errors without opening a browser.

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

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
