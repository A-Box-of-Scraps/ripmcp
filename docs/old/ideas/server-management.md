# Server management

Status: proposals and open questions.

## Installation

Separate two meanings:

- Registration: save a named command or remote endpoint plus its configuration.
- Provisioning: download packages, select versions, and manage runtime dependencies.

User clarification: installation must cover both remote JSON configuration and
local servers using npx, uvx, or Docker. Registration-only scope is insufficient.

Proposal: save the server definition and delegate local provisioning and launch to
the selected runtime. Do not build a replacement package manager. Version pinning
and runtime prerequisite handling remain open.

Agreed: installation prepares and validates by default, with an explicit flag to
skip verification. Proposed spelling: --skip-verify. Verification checks the
connection and tool discovery without invoking tools. Skipping verification does
not implicitly skip dependency preparation. Failure and rollback behavior remain
to be decided.

Agreed: ordinary uninstall removes the definition and stops owned processes,
without deleting caches or server data.

User proposal: an explicit --clean flag requests additional cleanup, with an
interactive confirmation and -y for unattended use.

Proposed safety contract, pending agreement:

- Show the exact resources to delete before confirmation; default to [y/N].
- Delete only resources tracked as exclusively owned by this server installation.
- Preserve shared runtime caches, shared images, external directories, bind mounts,
  and resources with uncertain ownership. Report what was preserved and why.
- -y skips confirmation only; it does not expand cleanup scope.
- Without an interactive terminal, --clean requires -y or fails before changes.
- Confirm before stopping the server, deleting resources, or removing its definition.
- Retain enough ownership metadata to retry cleanup after a partial failure.

This requires installation-time resource tracking, not guessing ownership during
uninstall. Cleanup is best-effort removal of owned resources, not a guarantee that
all traces of arbitrary server software are erased.

Agreed: owned data means resources ripmcp created and tracks for its exclusive use, such
as a dedicated installation directory or its own logs. A user-supplied directory,
shared runtime cache, or existing Docker volume is not owned merely because a
server uses it. Preserve resources whose ownership is uncertain.

Agreed: partial failures must be explicit and diagnosable, never silent. Proposed
report: completed actions, failed actions and causes, preserved resources, and
retry instructions, with a nonzero exit status when requested cleanup fails.

## Enablement

Proposal: disabled servers are excluded from discovery and invocation; direct
calls fail clearly rather than silently enabling them.

Agreed: per-tool enable/disable is required in v1. Disabled tools are excluded from
normal discovery and blocked on direct invocation through ripmcp. Hiding a tool
alone is insufficient. These controls apply to ripmcp, not other clients accessing
the server.

Proposal: identify tool settings by server and tool name, persist them across
restarts, and provide an explicit listing option to inspect disabled tools.
A disabled server takes precedence over individual tool settings.

Agreed: default allow. Existing and newly discovered tools are enabled unless
explicitly disabled. Users selectively disable unwanted tools; new tools do not
require approval. A separate explicit-selection policy is not part of v1 scope.

Disabling configuration and stopping a process are separate operations. Decide
whether disable also stops an owned process.

## Local lifecycle

User direction: persistent local servers with start, repeated calls, and stop.
Per-command processes are not the desired default.

The MCP stdio binding uses a client-launched subprocess and its standard streams.
The 2026-07-28 protocol does not require a persistent connection-scoped session;
process lifetime and protocol session semantics are distinct.

Reference: official MCP 2026-07-28 specification, Basic / Transports / stdio.

Proposal: a small background supervisor owns local processes and their streams,
while short-lived CLI commands communicate with it. This is an architectural
proposal, not yet an agreed implementation. It needs concurrency rules, crash
recovery, and cleanup; keeping only a PID is insufficient for stdio reuse.

Agreed: tool invocation auto-starts enabled local servers and reuses them across
calls. Explicit start and stop remain available. Disabled servers never
auto-start. Automatic idle shutdown is deferred; servers remain until stopped.
Remote endpoints have no start/stop operations.

Only stop processes owned by ripmcp. A remote endpoint can be enabled or disabled
locally without claiming to control the remote service.

## Status and configuration

Proposal: keep configured enablement, owned-process state, and observed health
separate. An enabled server is not necessarily running or reachable.

List saved configuration without launching servers by default. Make active health
checks explicit and distinguish unknown health from failed health.

Open questions:

- User-level configuration only, or project-level overrides too?
- How is trust granted before running a command from project configuration?
- What authentication is required for the first real remote servers?
- Where do logs live, and how are secrets redacted?
