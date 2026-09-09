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

| Code | Meaning                                                    |
| ---- | ---------------------------------------------------------- |
| 0    | Success (also help/version)                                |
| 1    | Internal or I/O failure, including stdout write failure    |
| 2    | Invalid CLI or tool input                                  |
| 3    | Configuration, policy or project trust failure             |
| 4    | Connection/transport failure                               |
| 5    | Protocol violation, negotiation or protocol error response |
| 6    | Authentication required or authentication failure          |
| 7    | Tool-reported failure (`isError: true`)                    |
| 8    | Partial discovery or incomplete requested cleanup          |
| 9    | Deadline exceeded                                          |
| 10   | Unsupported platform/capability or unimplemented handler   |
| 130  | Cancelled by user                                          |

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

D10's runtime fallback is `/tmp/ripmcp-<numeric-effective-uid>/`, ignoring `TMPDIR`.
Phase 02 passed directory creation, private ownership/mode, symlink and foreign
owner rejection, and locking tests. Phase 04 must still verify socket peers and
stale-socket handling before using this directory for supervisor communication.

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

## Phase 02 concrete configuration and storage contracts

Recorded September 8, 2026 as implementation choices under D01-D03/D09-D10.
Executable schemas and tests, rather than these notes, define field validation.

- User/project documents share `Configuration`: required `schema_version: 1`,
  optional `servers` map and optional non-null `timeouts`. Unknown fields,
  duplicate map entries, trailing documents and unsupported versions fail closed.
- A native server object has `enabled` (default true), `disabled_tools` (default
  empty), and required `definition`. Local definitions contain `kind: "local"`,
  `runtime: "npx" | "uvx" | "docker"`, `package`, `transport: "stdio"`, optional
  `args`, reference-only `env`, and optional absolute `cwd`. Remote definitions
  contain `kind: "remote"`, HTTP(S) `url`, `transport: "streamable_http"`, optional
  reference-only `headers`, and `authentication: "none" | "oauth"` (default none).
  No generated OAuth state or inline credential field exists in these schemas.
- Secret references are one-key objects. Environment names use the foundation
  grammar. Keyring identifiers use nonempty ASCII letters/digits/underscore/dot/
  hyphen, excluding `.` and `..`; they are opaque IDs, not paths or URLs. Generic
  keyring resolution currently fails unsupported. Phase 05 must implement secure
  storage and independently validate OAuth resource/issuer/client binding; an
  opaque lookup ID alone must never authorize OAuth credential reuse.
- Discovery canonicalizes the working directory, then visits every ancestor to
  filesystem root. A `.ripmcp` directory without `config.json` does not select a
  project. A selected malformed configuration is an error. Configuration storage
  paths/files reject symlinks; a symlink alias of a project working directory
  resolves to the same canonical root, not a new approval identity.
- Effective server identity includes canonical scope/provenance, name and SHA-256
  of the exact source bytes. Overrides replace the complete server object. Project
  timeouts replace user timeouts only after approval. Passive reports omit
  definitions, endpoints, arguments, environment values and credential references.
- Trust state is private `XDG_STATE_HOME/ripmcp/trust.json`, with schema version
  and canonical-root-to-content-digest approvals. State under the selected project
  root is rejected. Previews expose runtime/package or endpoint origin, policy,
  reference field names and content/endpoint digests. Arguments, reference IDs,
  URL paths/queries and URL package details are conservatively redacted, with an
  explicit instruction to inspect the local configuration before approving.
- Each operation must load a fresh effective snapshot. `authorize` rejects an
  untrusted override without falling back; the resulting handle exposes only that
  parsed snapshot. Enablement/tool checks are separate so stop/status can later
  inspect a disabled server without allowing execution. Secret resolution requires
  an authorized, enabled handle. Approvals never reread bytes after preview.
- Scope writes default to user; project writes require a discovered, trusted,
  unchanged selection. Mutations reread under the target file lock, reject duplicate
  registration, report trusted project shadowing, and report reapproval when
  project bytes actually change. No-op writes do not invalidate identical content.
- Reads do not create directories. Absolute valid XDG overrides win; empty/relative
  overrides use HOME defaults. Missing HOME is an error only when a needed path
  lacks its own override. Runtime resolution does not require HOME. New directories
  are 0700; sensitive state and locks require the effective UID and exact 0700/0600
  directory/file modes. Existing unsafe state is rejected, not silently chmodded.
- Files are opened relative to no-follow directory descriptors, bounded to 4 MiB,
  and checked for regular-file type and a single link. Per-file `.NAME.lock` files
  serialize cooperating writers. Writes use an exclusive `.NAME.pending`, file
  fsync, rename in the same directory, and directory fsync. An interrupted pending
  file never becomes authoritative; a subsequent changed write removes it while
  holding the lock. Corrupt/incompatible committed state is preserved and errors.
- Storage APIs accept an existing operation deadline; convenience operations use
  60 seconds, while scoped writes use effective operation settings. Lock acquisition
  and precommit checks consume that deadline. Synchronous filesystem system calls
  are not preempted; signal/async integration remains phase 03 work.
- `ownership.json` is a private versioned journal, independent of configuration
  registration. Installation IDs are immutable 128-bit random lowercase hex IDs.
  Records retain scope, origin/resolution, registration state, typed tracked path
  or runtime resources, ownership classification, retained-data references, cleanup
  states and operation intents/progress. Unregistering retains records and retries;
  paths need not still exist on journal reload. No recursive resource scanning or
  actual installation/deletion is introduced in phase 02.

## Phase 03 client integration contracts

Completed September 8, 2026. The exact compatibility profile, pinned upstream
audit and test evidence are in `03-protocol-client.md`.

- `mcp::Client::{stdio,http}` first perform modern `server/discover`, not legacy
  initialization. Only MCP `2026-07-28` is accepted. Each request carries its own
  version/capabilities; HTTP has no protocol sessions in this revision.
- Pass an existing `Deadline` and per-operation cancellation token in `Operation`.
  Reuse that operation through discovery and invocation. Keep local clients under
  supervisor ownership; a cancelled call does not stop the shared process.
- Callers remain responsible for fresh configuration/trust and enablement checks,
  authorized environment/secret resolution, and per-tool policy. No MCP runtime
  types or connections have been embedded in CLI parsing/dispatch.
- Use `Discovery::require_complete` before cross-server uniqueness resolution.
  Invalid HTTP annotation definitions are explicitly reported and excluded; failed
  pages return errors, not successful partial/empty lists. Qualified lookup can
  select a valid tool despite unrelated rejected definitions. No discovery cache
  exists and each call fetches fresh metadata under the same deadline.
- Serialize the `ToolResult` or its envelope directly; inspect `is_error()` for
  exit 7 after successful output. Do not replace the result with text-only content
  or conflate a tool failure with a protocol error. Do not replay on any failure.
- Use `json::parse` for arbitrary JSON inputs/results. It retains exact numbers
  and literal private-looking field names, rejects duplicate keys and bounds
  nesting. Inline arguments already use it; file/stdin and shape integrations
  should not bypass it with generic `Value` deserialization.
- Authentication providers must bind credentials/challenges to the supplied exact
  resource, obey the operation deadline, sanitize diagnostics and remain
  noninteractive. Phase 05 supplies OAuth and secure storage. Authorization
  references must use this provider path rather than extra HTTP headers.
- `SignalCancellation` is for the short-lived CLI lifetime. The supervisor should
  translate IPC cancellation to individual operation tokens, not forward SIGINT
  to its shared MCP child. Async stream cancellation and process shutdown have
  separate lifetimes; process-group ownership remains phase 04 work.

## Phase 04 supervisor integration contracts

Completed September 8, 2026. The phase handoff contains the test matrix and exact
validation commands. These choices implement D01/D07/D10 without adding product
documentation or completing the installation, authentication or cleanup phases.

- Local CLI start/stop and qualified tools/tool/call use `supervisor::Connection`.
  Existing-only connection/status never starts the daemon. Lazy bootstrap uses
  the authenticated private endpoint; protocol version 2 is not compatible with
  the earlier control-only version 1. Unsafe or incompatible artifacts are
  preserved rather than treated as permission to kill a recorded PID.
- Daemon and instance lifetime locks live in private state storage, independently
  of socket placement. Changing XDG_RUNTIME_DIR cannot start a second owner of the
  same state. Instance keys bind canonical scope/name; reuse additionally requires
  the same installation ID, source revision, resolved origin, runtime executable
  metadata and effective child environment. Mutable ownership bookkeeping is not
  a process revision and does not discard a successfully verified connection.
- A prepared local `Origin::Local` requires matching runtime/request, a resolved
  immutable package/image and an absolute `executable`. This additive optional
  origin field preserves existing journal decoding; missing preparation fails
  closed. Phase 06 must populate it and must not use start as a package resolver.
- The supervisor independently reloads trust, enablement and tool policy, including
  after both request-queue and ownership-lease waits. A final check separates tool
  discovery from invocation. Stop uses only the previously owned scoped process;
  it does not execute an edited command or dereference its new credentials.
- One broker operation runs per scoped slot. Limits are 16 admitted operations per
  slot, 256 slots, and 32 IPC connections. Each supervised MCP connection reserves
  at most 16 unsettled request permits. Cancelled requests retain only bounded
  admission until a response or transport closure, rather than allowing unlimited
  orphan work. Subsequent calls can use remaining permits without killing or
  replaying the cancelled call. All admission and IPC waits consume the original
  monotonic operation budget. Initialization/control reads have a 60-second cap.
- Operation task lifetime is independent of the CLI socket. Disconnect cancels
  the operation, but a cancelled startup finishes owned teardown while retaining
  its maintenance read guard. Shutdown cancels operations, drains the barrier and
  stops managed children. Phase 08 must execute mutations under the exclusive
  supervisor-owned barrier, not an expiring external lease.
- Child guards monitor supervisor pidfds and retain unreaped group leaders until
  group TERM/KILL completes. They inherit ownership leases into descendants and
  reap adopted children. Runtime failure can restart on the next explicit use;
  calls with uncertain delivery are never automatically replayed. A held stale
  lease is not permission to signal a PID found on disk. Kernel pidfd support is
  required and checked before bootstrap.
- Docker ownership uses an immutable image, unique name/label and verified
  container ID/name/label before removal. Private `.instance-<key>.failed` state
  records are durable before launch and bind installation, runtime and effective
  revision. Failed cleanup preserves the record and prevents an unsafe relaunch.
  Phase 08 must reconcile these records, including resource/credential context,
  before deleting installation data or declaring cleanup complete.
- Child environment is a small caller base plus freshly authorized references;
  it is not the supervisor's inherited credential snapshot. Guard runtime/state
  paths are separate from child environment overrides. User cwd defaults to `/`,
  project cwd to the canonical project root; explicit absolute cwd takes priority.
- Server reports retain name/provenance/enablement/trust fields and add
  `process_state` and `health`. Remote entries are `not_managed`/`unknown`.
  Config-revision or age changes make observations stale; status and a reused
  start do not refresh health. Unverified, unknown, stale and failed are distinct.
  No idle shutdown, background probe, result cache or truncation was introduced.

Primary OS/runtime references checked for the ownership implementation:

- `https://man7.org/linux/man-pages/man2/pidfd_open.2.html`
- `https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html`
- `https://docs.docker.com/reference/cli/docker/container/ls/`
- `https://docs.docker.com/reference/cli/docker/inspect/`

The pidfd zombie-retention precondition is enforced with an installed SIGCHLD
handler and no reaping before group signals. The pinned signal-hook-registry
implementation replaces SIGCHLD flags without SA_NOCLDWAIT. Default validation
uses fake runtime executables, not a real Docker daemon or package registry.

## Phase 05 authentication integration contracts

Completed September 8, 2026. The exact authorization profile, library audit,
provider limitations and test matrix are in `05-authentication.md`.

- Login is explicit and uses the published HTTPS client metadata identity, S256
  PKCE and the fixed loopback redirect. Native public DCR is a compatibility
  fallback; pre-registration-only and private-network OAuth providers are
  unsupported. No device flow, remote-headless redirect or implicit login exists.
- Use `auth::remote::options` for remote installation verification and qualified
  operations. Its provider rechecks configuration/trust before credential access
  and before transmission. Never attach OAuth tokens as configured extra headers,
  substitute another endpoint, or replay a request after a challenge/failure.
- OAuth records bind endpoint, canonical resource, exact issuer, client ID and
  token endpoint. Identical endpoints share records across names and trusted
  scopes, not across resources or issuers. Status is local and explicitly does
  not claim remote validity; logout affects every alias for that endpoint.
- Secret Service's unlocked default collection and encrypted session are required.
  There is no plaintext fallback. OAuth and generic keyring references use
  separate `oauth-v1` and `references-v1` attribute namespaces under
  `application=ripmcp`; `identity` is respectively the endpoint hash or opaque ID.
  Provisioning generic keyring secrets remains external; reads never unlock or
  create a collection. Use `AuthorizedServer::resolve_secrets_async` for real
  keyring references; the synchronous environment-only backend fails explicitly.
- Login generation checks, per-endpoint cross-process locks and non-secret epoch
  files prevent logout/refresh races and late cancelled D-Bus writes from restoring
  old credentials. Locks/epochs use the validated per-user `/tmp` directory,
  independently of XDG overrides, because the OS keyring is shared by the user.
  Tokens themselves never enter those files or project configuration.
- Refresh invalidates the old generation before sending, atomically replaces the
  complete credential payload, and never retries an uncertain refresh. Any failed
  refresh/persistence requires explicit login. OAuth logout replaces the secret
  payload with an empty tombstone; it does not promise provider-side revocation or
  cancellation of already-authorized requests. Later lookups always reread state.
- Phase 06 must use the noninteractive provider for installation verification and
  record the effective remote identity for later ownership/cleanup work. Do not
  treat an auth nickname, opaque keyring ID, saved status or stale config snapshot
  as permission to transmit credentials.

## Phase 06 installation integration contracts

Completed September 8, 2026. See `06-installation.md` for runtime resolution,
transaction failure semantics, source references and validation evidence.

- `install` is implemented for the four approved forms. Native imports are bounded,
  regular-file, strict server objects; project-origin imports require unchanged
  trust even when importing into user scope from another directory. Skip-verify
  never bypasses transport/header/OAuth-profile or scope/ownership validation.
- Supervisor IPC version 3 adds typed installation requests. The supervisor owns
  provisional local children and retains only successfully verified local MCP
  processes. Remote verification clients live only for their install request.
- `Target` and `LockedStore` retain a configuration lock through preparation and
  verification. Staged definitions cannot be used by ordinary commands. The
  configuration rename publishes a previously journaled prepared installation;
  snapshots and intent/progress records resolve interrupted publication. A failed
  late write is an uncertain commit, not authority to restore old config bytes.
- `OwnershipStore::update_async` uses cancellation-aware lock acquisition and the
  existing operation budget. Installation records add optional definition and
  intended-config-digest snapshots plus `unknown`/`unverified`/`verified` install-time
  verification. Existing records still decode. Later observed health remains a
  separate supervisor concern.
- Install operation resources include `ripmcp-preparation-lease` and
  `ripmcp-instance-lease` identities for local work. Recovery must reconcile those
  with any `.instance-<key>.failed` container record and actual held leases. Never
  infer PID authority from a journal or remove shared caches/images/user paths.
- uvx launch now uses its documented `--from <exact-requirement> <command>` form,
  rather than treating a requirement string as an executable name. Python downloads
  remain disabled. No launch resolves an unpinned top-level package or image.
- A failed remote auth check does not publish a pending active registration. The
  recovery path is validated skip-verify registration, explicit auth login and
  discovery, with project reapproval and shadowing caveats. Secret values never
  enter installation reports or ownership snapshots.

## Phase 07 tool-workflow integration contracts

Completed September 8, 2026. See 07-tool-workflow.md for regression evidence.

- Policy mutations use the selected scope and compare the target definition again
  under its configuration lock. A retained disabled name is a known policy target
  even after discovery drops it. Other tool targets require live discovery; disabled
  servers cannot be temporarily enabled for validation. If a project shadows a user
  target requiring discovery, run the mutation outside that project. No-op edits
  preserve original bytes, avoiding accidental project-trust invalidation.
- Tool lists use `{schema_version:1,tools:[...],errors:[...]}` with entries containing
  `server`, `name`, `enabled`, and optional `description`/`title`. `--all` requires a
  server and includes disabled tools, never disabled servers. Tool detail preserves
  the full tool definition under `tool`. Partial discovery exits 8 after writing its
  valid entries and safe error identities/codes. Empty complete discovery succeeds.
- Shorthand performs uncached discovery of every configured enabled server, including
  trust/auth failures as incomplete rather than treating those servers as empty.
  The effective identity set must remain unchanged during discovery. Exactly one
  enabled match permits dispatch; zero matches exit 2, collisions exit 2 with
  `{schema_version:1,candidates:[{server,tool},...]}`, and incomplete discovery exits
  8 with the discovery report. This is an observed discovery snapshot, not an atomic
  lock on remote servers' future metadata. Qualified calls discover only their target.
- Policy/trust checks at dispatch are authoritative; disabling during discovery
  prevents a call. A request already transmitted may complete and returns its full
  result even if disabled meanwhile. No cancellation, revocation, replay, result
  cache, automatic save, or preview invocation is implied.
- Arguments require exactly one strict JSON object. File/stdin input is bounded to
  16 MiB, duplicate keys and nesting at depth 128 are rejected, and exact numbers
  are retained. General JSON Schema validation remains server-side; HTTP transport
  annotation types/values are validated by the existing protocol client. Parsing,
  read, size, and header-validation errors occur before tool invocation and never
  echo input. Input is read before shorthand discovery.
- Shape accepts arbitrary JSON roots from a saved file or stdin. It has no config,
  supervisor, trust, or credential dependency at runtime. Its report is
  `{schema_version:1,shape:...,limits:...}`. Scalar nodes contain only `type`;
  object nodes add `fields`, array nodes add exact `length` and ordered `items`
  containing each inspected element's shape. It does not merge heterogeneous
  elements or claim unseen elements have inspected types.
- Shape input is limited to 64 MiB and the strict parser's nesting bound of 128.
  Inspection defaults to depth 8 and width 100, with CLI maxima 64 and 10000.
  Root depth is zero; a container at the selected depth retains type/array length
  but omits children. A global 100000-node inspection budget also applies. Each
  container reports `truncated`; omitted immediate fields/elements have an exact
  `omitted` count. Input size/nesting rejection exits 2 with no shape output;
  bounded successful inspection exits 0 and never modifies the saved result.
- Shape uses the explicit timeout or a standalone 60-second default, not configured
  settings. Streaming reads observe timeout/SIGINT. Synchronous parsing/regular-file
  reads cannot be preempted; the deadline is checked before output.
