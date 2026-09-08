# 04: Persistent supervisor, local lifecycle and status - 2026-09-08

Status: **Done**

Dependencies: 02, 03.

Contract gates: D01, D07, D10; process retention and shutdown contracts.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/supervisor/ (new or modify when introduced by an earlier phase)
+ src/cli.rs (new or modify when introduced by an earlier phase)
+ tests/supervisor.rs (new or modify when introduced by an earlier phase)
+ tests/lifecycle.rs (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Implement a lazily started per-user supervisor and versioned IPC protocol over a private Unix socket. Handle concurrent bootstrap with locking, socket ownership checks and peer authentication. Reject incompatible clients and unsafe/stale socket reuse.
2. [x] Key managed instances by installation/scope identity and effective configuration revision. Reuse live MCP stdio connections across CLI processes. Prevent same-named user/project servers and changed definitions from accidentally sharing a process or credentials.
3. [x] Implement per-server start serialization, bounded request queues and the selected concurrent-call policy. Do not let a disconnected CLI leave an unbounded orphan request. Enforce current trust/enablement rather than trusting an old snapshot from a client.
4. [x] Implement explicit local `start` and `stop`, plus an internal ensure-running path for enabled local calls. Reject remote start/stop and disabled-server starts. Stop only owned processes/containers, with graceful shutdown followed by bounded termination of the owned process group.
5. [x] Implement crash detection, stale operational-state reconciliation, and restart-on-next-use behavior without replaying interrupted calls. Guard against PID reuse; arrange child teardown on supervisor death or safely reconcile surviving owned children without double launching.
6. [x] Implement passive `servers` output separating configured enablement, local process state and observed health. Remote status is not a claim that ripmcp controls the endpoint. Keep unknown/unverified/stale health distinct from failed health.
7. [x] Add shutdown coordination and a maintenance barrier for phase 08 so cleanup cannot race an auto-start or active mutation. No automatic idle shutdown in v1.

## Acceptance criteria

- Two separate CLI calls reuse one server; concurrent first calls launch exactly one instance.
- Disabled/untrusted servers cannot start via any path; remote start/stop fail clearly.
- Stopping one scoped server never kills another or a foreign process with a reused PID.
- Supervisor/child crashes, stale sockets, IPC disconnects and config changes have recovery tests.
- `servers` launches nothing; processes remain available until explicit stop or failure.
- Repository validation gate passes.

## References

server-management.md; index.md agreed lifecycle; configuration.md runtime storage and trust.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: Steps 1-7 and the acceptance criteria. Persistent local MCP
  connections now back start/stop and qualified local tools/tool/call commands.
  Server calls auto-start only after fresh trust, policy and installation checks.
  Status is passive and reports enablement, process state and health separately.
- In-progress task: None in phase 04. Remote command integration and cross-server
  resolution remain in phases 05/07; provisioning remains phase 06; cleanup and
  its confirmation/reporting workflows remain phase 08.
- Changed paths / commits: IPC foundation commit `e13237b` was committed and pushed
  before this continuation. Lifecycle completion is in the commit containing this
  handoff. Changes cover `src/supervisor/`, CLI dispatch, MCP cancellation/lifetime
  support, configuration identity access, the local ownership origin's optional
  runtime executable, storage helpers, lifecycle/IPC fixtures and regression tests.
  Cargo enables rustix's monotonic clock feature without changing pinned versions.
  No README, product documentation or lint implementation changes.
- Tests run and results (September 8, 2026):
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: passed, 130 tests across root-package targets.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
  - `for run in 1 2 3 4 5; do cargo test -q --test lifecycle --test supervisor || exit; done`:
    passed five consecutive runs, 27 lifecycle and 14 supervisor tests per run.
  - Tests use isolated private HOME/XDG roots and fake npx/uvx/Docker executables.
    No public endpoints, personal credentials, package downloads or real runtime
    provisioning were used. Real-runtime smoke tests were not run.
- Decisions and concrete contracts:
  - IPC is version 2. The handshake checks protocol/build, instance nonce, private
    socket identity and kernel peer credentials. Bootstrap retains runtime-local
    locking; daemon and instance lifetime locks live in private state storage so
    changing XDG_RUNTIME_DIR cannot create duplicate owners of the same state.
    Incompatible/unrecorded socket artifacts are preserved and rejected.
  - Managed identity includes canonical scope/name, immutable installation ID,
    source configuration digest, resolved origin, runtime executable metadata and
    effective child environment. Ownership bookkeeping does not change a process
    revision. Changed definitions, credentials or installations cannot reuse the
    old connection. Replacements stop the old owned process before launching.
  - The supervisor reloads configuration/trust/policy before admission, after
    queueing and ownership-lease waits, after initialization, and before actual tool invocation following
    discovery. Stop never executes a newly edited definition or resolves secrets;
    it may stop an already-owned disabled or now-untrusted scoped instance.
  - One active broker operation per scoped server; at most 16 admitted operations
    per slot, 256 slots and 32 IPC connections. Supervised MCP connections admit
    at most 16 unsettled requests, including abandoned requests. Cancellation
    releases the CLI operation, sends protocol cancellation and retains only the
    bounded unsettled-request admission until a reply or transport close. Later
    calls can reuse the process after one ignored cancellation; exhaustion waits
    within the caller's deadline. No invocation is automatically replayed and one
    cancelled call does not kill a reused process.
  - Operation budgets cross IPC using Linux's shared monotonic clock and retain
    queue/transfer time. Handshakes are capped at 60 seconds and 4 KiB; operation
    frames have a 64 MiB ceiling, separate from MCP/input limits. Results are not
    truncated and retain exact JSON numbers and extension fields. File/stdin
    local call input is bounded; pipe waits are deadline/cancellation aware.
  - A separate child guard monitors the supervisor through a pidfd. It retains
    the process-group leader unreaped while sending TERM/KILL, then reaps owned
    children. Instance leases remain inherited by descendants to fence unsafe
    relaunch after guard failure. The manager rechecks policy after waiting for a surviving owner; the guard
    acquires its lease without deferring execution behind another ownership wait.
    No persisted PID is used as killing authority.
    Missing kernel pidfd support fails closed. This is ownership management, not
    a sandbox for arbitrary server code.
  - Docker starts use a unique name/ownership label, immutable image digest,
    --rm/--init, and name-only environment forwarding. Teardown verifies container
    ID/name/label before removal. A durable private state record is written before
    launch, includes installation/revision identity, and remains on failure. Stop
    reports incomplete cleanup and later launch cannot guess away that ownership.
  - `servers` never bootstraps, invokes MCP, resolves references or refreshes a
    health observation. Running, stopped, busy, failed and unknown process states
    are distinct from healthy, failed, unknown, unverified and stale health.
    Remote entries report not_managed/unknown. Held orphan leases are unknown,
    not proof that a process is actively stopping. Cleanup failures stay visible.
  - All local operations hold the maintenance barrier. Cancelled startup tasks
    finish owned teardown before releasing it. Shutdown cancels requests, drains
    the barrier and stops managed children before removing the endpoint. Its IPC
    acknowledgement means stopping, not a completed cleanup report. No idle
    shutdown was added.
- Downstream integration requirements:
  - Phase 06 must record a registered local origin with matching runtime/request,
    immutable resolution and absolute `executable`. No floating-name fallback or
    on-demand provisioning is implemented by start. Successful verification can
    use the same supervisor API and retain its process through bookkeeping.
  - User definitions without cwd run from `/`; project definitions default to
    the canonical approved project root. An explicit absolute cwd wins. Child
    environment is a small caller-supplied base plus freshly authorized references;
    guard runtime/state paths are separate from that environment.
  - Phase 05 must replace the intentionally unsupported keyring backend and wire
    remote authentication without inheriting old daemon credentials.
  - Phase 08 must run mutations under the supervisor-owned exclusive barrier,
    stop/check owned leases and durable retry records before deletion, and keep
    installation identities distinct. Do not introduce a client-side maintenance
    lease that expires while an external mutation could continue.
- Blockers / remaining questions: None for phase 04. Later-phase integrations
  above are not implemented or claimed complete by this phase.
- Next action: Proceed to phase 05's authorization and secure-store verification
  gates, using the completed phase 03 transport and phase 04 lifecycle contracts.
