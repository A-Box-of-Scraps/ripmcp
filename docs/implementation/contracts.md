# Phase 01 implementation contracts

Recorded September 8, 2026 after the user approved D01-D10 and authorized completion
of the foundation. These are engineering choices under that approval, not new
user-supplied requirements. The authoritative command surface is in commands.md.
Implementations in later phases must either follow these choices or explicitly
record a revised decision before dependent work. Pending security/protocol gates
below are not approvals of an unverified implementation.

## Timeouts and cancellation

- Native config version 1 reserves top-level `timeouts`:
  `{"operation_seconds":60,"login_seconds":300}`. Fields are positive u64 integers;
  omitted fields take these defaults. Zero, null, fractions and unknown fields fail.
  `src/deadline.rs::Timeouts` is the executable schema for this section.
- Select user settings, then replace the entire timeout section if a trusted
  selected project supplies one. Untrusted settings cannot affect deadlines.
  Explicit CLI `--timeout` overrides both. Login uses `login_seconds`; other
  bounded operations use `operation_seconds`. No server-specific timeout in v1.
- Establish one monotonic deadline per operation, not per request/page/retry.
  Queueing, preparation, connection, discovery and invocation consume that same
  budget. Do not reset it on progress. Positive u64 CLI durations must not overflow
  platform `Instant` arithmetic; `Deadline` uses elapsed duration subtraction.
- Confirmation waits occur before the mutation deadline starts. Explicit browser
  login's deadline includes user authorization and secure credential persistence.
- Phase 03 adds asynchronous deadline enforcement and signal propagation. On
  cancellation (SIGINT), request protocol cancellation where supported, release
  request resources, and exit 130. Do not kill a reused server for one cancelled
  call. A timeout exits 9. Neither implies a tool's side effects were rolled back.
- Never automatically replay a tool invocation after uncertain delivery. These
  contracts do not add runtime handlers or pretend placeholder handlers time out.

## Exit codes and output

| Code | Meaning |
| --- | --- |
| 0 | Success (also help/version) |
| 1 | Internal or I/O failure, including stdout write failure |
| 2 | Invalid CLI or tool input |
| 3 | Configuration, policy or project trust failure |
| 4 | Connection/transport failure |
| 5 | Protocol violation, negotiation or protocol error response |
| 6 | Authentication required or authentication failure |
| 7 | Tool-reported failure (`isError: true`) |
| 8 | Partial discovery or incomplete requested cleanup |
| 9 | Deadline exceeded |
| 10 | Unsupported platform/capability or unimplemented handler |
| 130 | Cancelled by user |

- Every data response is a single complete JSON value plus newline on stdout.
  Tool calls preserve the entire MCP result envelope, including extensions, and
  exit 7 on tool-reported failure. Output errors take precedence over exit 7.
  Full protocol envelope validation belongs to phase 03, not the output writer.
- Reserve versioned reporting objects for non-call handlers: `schema_version: 1`
  plus command-specific fields (`servers`, `tools`, `tool`, `auth`, `trust`, or
  `actions`). Partial results contain `errors` with safe code/identity diagnostics;
  cleanup also contains `preserved` and `retry_required`. Concrete field models
  and snapshot tests are required when each handler is implemented. Do not wrap
  MCP tool-result envelopes with this reporting version.
- Diagnostics, previews, confirmation prompts and browser login instructions use
  stderr only. Errors never echo raw arguments, JSON, environment values, bearer
  tokens, authorization codes, PKCE verifiers, client secrets or upstream errors.
  Keep safe static diagnostics at the entry point; map causes to typed errors.
- The validated browser authorization URL printed during explicit login is a
  deliberate exception to URL redaction required by D08. Do not log it or print
  token/callback URLs. Safe field names, server identities and cleanup target paths
  may be displayed with control characters escaped and credential parts removed.
- Shape reports names, types and array lengths, not scalar values. Depth and width
  bounds affect inspection only; each omitted subtree/entry set must be marked
  truncated, with counts when known. It neither caches nor truncates tool results.

## Installation and lifetime boundaries

- Preserve the requested package/image spec and record the resolved immutable
  version or digest and runtime identity in the installation record. Resolve during
  preparation using the existing runtime; later starts use the recorded resolution.
  Do not silently float versions on start or install runtimes. Fail preparation if
  a stable resolution cannot be obtained. Runtime-specific resolution and sandboxed
  fake-runtime tests are phase 06 work, not grounds to guess a resolved version.
- Prepare even with `--skip-verify`. Verification connects and discovers tools;
  it never invokes a tool. A successful verified local installation retains its
  supervisor-owned process for later calls. Skipped verification does not start it.
  Failed installation stops only newly owned processes, rolls back its registration,
  and preserves durable retry records for owned resources not removed successfully.
- Each CLI invocation owns its remote client, outstanding requests and any negotiated
  remote session. Release request/session resources on completion as required by the
  audited transport. Do not persist a remote session in the local supervisor or assume
  that remote sessions are required. Server lifetime is never controlled remotely.
- Local transports must remain independently testable and transferable to supervisor
  ownership. No per-command local spawning or concrete protocol SDK type is baked
  into the CLI request model.

## Credential references and diagnostic trust

- Native server secret fields use one-key reference objects, `{"env":"NAME"}` or
  `{"keyring":"opaque-id"}`. No inline secret-value alternative. Environment names
  must match `[A-Za-z_][A-Za-z0-9_]*`; keyring IDs are nonempty opaque lookup keys, not
  paths, URLs or serialized secrets. Phase 02 validates reference syntax without
  dereferencing it. Keyring backend selection remains gated by D08 in phase 05.
- Generated OAuth credentials and their references live outside project config.
  Store them against canonical resource, issuer and client identity, not a display
  name. A lookup key is not authority: validate stored resource/issuer binding on
  every use. Trust must be checked before resolving any project reference.
- Passive `servers` may report untrusted definition names, provenance, configured
  enablement and `trust_required` state, but not command arguments, environment
  values, credential references or raw endpoint URLs. It must not resolve secrets,
  connect, spawn, create state or silently fall back to a shadowed user definition.
- `trust` reads and previews the exact selected configuration's execution/endpoint
  effects, with sensitive values redacted, then asks `[y/N]` on a terminal. With no
  terminal, fail before writing approval and instruct the caller to run it in a
  terminal. No `-y` spelling is introduced. Save approval only for the previewed
  content bytes and canonical root; configuration changes require renewed approval.

## Verified sources and explicit downstream gates

Foundation dependency selection uses clap derive, serde, serde_json and test-only
TempDir fixtures; Cargo.lock pins the resolved builds. No MCP or OAuth library is
selected. This keeps the foundation independent of protocol-library coverage.

Official sources inspected on September 8, 2026:

- `https://modelcontextprotocol.io/specification/2026-07-28` identifies
  `2026-07-28` as the latest revision.
- `https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/crates/rmcp/src/model.rs`
  defines `V_2026_07_28`, but `ProtocolVersion::LATEST` still selects `V_2025_11_25`.
  A version constant alone does not establish compatible defaults or coverage.
- `https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/crates/rmcp/Cargo.toml`
  lists client, child-process, Streamable HTTP and authorization feature areas.
- `https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/crates/rmcp/src/transport/auth.rs`
  contains PKCE, issuer checking and registration/discovery code. Presence alone
  does not establish compliance with all requirements of the requested revision.

These are moving upstream sources, not pinned dependency audit evidence. Phase 03
must audit a concrete release/commit against the requested transport and capability
profile, including negotiation defaults. Phase 05 must verify authorization,
registration and secure-store behavior. Ask the user if the requested revision is
unavailable or incompatible; do not substitute an older revision. These affected
integrations remain gated, while phase 02 configuration work can proceed.

D10's runtime fallback candidate is `/tmp/ripmcp-<numeric-uid>/`, ignoring `TMPDIR`.
Phase 02 must test exclusive creation, private ownership/mode, symlink and foreign
owner rejection, and locking; phase 04 must verify socket peers and stale-socket
handling. This candidate is not approved for use until those security tests pass.

## Fixture boundaries

- `tests/support/mod.rs`: subprocess CLI runner with isolated HOME/XDG directories
  and an empty inherited environment. No public services or personal config.
- `tests/support/fixtures/process.rs`: temporary executable scripts named npx,
  uvx, docker or stdio; literal argument forwarding; child kill/wait on drop or
  deadline. Scripts must not launch untracked grandchildren. Output fixtures are
  intentionally small enough to fit pipes; large/fragmented transport readers
  and process-group ownership tests belong to phases 03-04.
- `tests/support/fixtures/http.rs`: loopback-only ephemeral listener, ordered raw
  responses, captured requests including Content-Length bodies, bounded reads and
  accepts, and close/malformed/stall fault injection. Drop stops and joins the worker.
- Current stdio fixtures replay a scripted JSON-RPC line. HTTP/OAuth tests exercise
  fixture transport and fault plumbing, not a compliant MCP or OAuth implementation.
  Full handshake, pagination, registration, callback validation and cancellation
  scenarios belong to phases 03 and 05 after the protocol audit.
