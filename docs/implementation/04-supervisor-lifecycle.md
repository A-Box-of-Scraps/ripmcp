# 04: Persistent supervisor, local lifecycle and status - 2026-09-08

Status: **In progress**

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
2. [ ] Key managed instances by installation/scope identity and effective configuration revision. Reuse live MCP stdio connections across CLI processes. Prevent same-named user/project servers and changed definitions from accidentally sharing a process or credentials.
3. [ ] Implement per-server start serialization, bounded request queues and the selected concurrent-call policy. Do not let a disconnected CLI leave an unbounded orphan request. Enforce current trust/enablement rather than trusting an old snapshot from a client.
4. [ ] Implement explicit local `start` and `stop`, plus an internal ensure-running path for enabled local calls. Reject remote start/stop and disabled-server starts. Stop only owned processes/containers, with graceful shutdown followed by bounded termination of the owned process group.
5. [ ] Implement crash detection, stale operational-state reconciliation, and restart-on-next-use behavior without replaying interrupted calls. Guard against PID reuse; arrange child teardown on supervisor death or safely reconcile surviving owned children without double launching.
6. [ ] Implement passive `servers` output separating configured enablement, local process state and observed health. Remote status is not a claim that ripmcp controls the endpoint. Keep unknown/unverified/stale health distinct from failed health.
7. [ ] Add shutdown coordination and a maintenance barrier for phase 08 so cleanup cannot race an auto-start or active mutation. No automatic idle shutdown in v1.

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

- Completed tasks: Step 1. Added the persistent control-plane daemon, lazy client
  bootstrap, and authenticated/versioned IPC. Added an in-process maintenance and
  shutdown barrier as groundwork for step 7, not its completed integration.
- In-progress task: Phase 04 remains incomplete. Steps 2-6 and the managed-operation
  integration of step 7 have not been implemented. Public `start`/`stop` handlers
  remain unsupported; `servers` retains its existing passive configuration report.
  No MCP process is launched or retained by this control-plane increment.
- Changed paths / commits: `src/supervisor/{mod,client,endpoint,service,wire}.rs`,
  `src/{lib,cli}.rs`, `src/storage/{directory,paths}.rs`, `tests/supervisor.rs`,
  `tests/supervisor/ipc.rs`, `tests/lifecycle.rs`, and phase trackers. No commits.
  No README, product documentation, dependency or lint implementation changes.
- Tests run and results (September 8, 2026):
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q --test supervisor --test lifecycle`: passed, 13 supervisor
    tests and 2 barrier tests. Concurrent bootstrap runs eight independent test
    subprocesses against the real CLI daemon.
  - `cargo test -q`: passed, 101 tests across root-package targets.
  - `cargo dylint --all -- --locked --all-targets`: passed after fixing three
    missing explicit types in the test cleanup helper.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
- Engineering choices under D01/D10: control protocol version 1 additionally
  checks the package version, a random instance nonce and the config/data/state
  storage context. Endpoints use the existing validated runtime directory and
  descriptor-relative access, 0600 sockets, kernel peer UID/PID checks, a bootstrap
  lock and a separate daemon lifetime lock. A private record binds the socket
  device/inode and daemon PID. Only a matching, connection-refused socket is
  reclaimed while holding the lifetime lock; unknown or incompatible artifacts
  are preserved and rejected. Publication stages the socket before exposing its
  final name. Recorded interrupted publication is recoverable; a crash before
  recording a staged socket leaves an unrecorded artifact that is preserved and
  requires inspection rather than guessed ownership.
- IPC boundaries: `Connection::existing` is passive; `Connection::ensure` creates
  or reuses the daemon via hidden `__supervisor`. Both authenticate before use.
  Control sessions admit at most 32 clients, use 4 KiB length-prefixed frames and
  a 60-second connection deadline. Current requests are ping and shutdown only.
  This frame limit is not a tool-result truncation policy. Bootstrap waits consume
  the caller's operation deadline. The daemon has no idle shutdown. Control
  disconnect, malformed/oversized input, incompatible handshakes, storage-context
  mismatch, stale sockets, PID/inode mismatch and connection saturation have tests.
- Shutdown/maintenance boundary: `Barrier` waits for active read guards before
  granting maintenance exclusivity, and permanently rejects work once closing.
  There is intentionally no IPC maintenance lease that silently expires while an
  external cleanup process could still mutate resources. Phase 08 must execute
  mutations under the supervisor-owned barrier or establish equivalent fencing.
- Blockers / remaining questions: No new user approval is requested for this
  increment. Child/process-group ownership and supervisor-death teardown are not
  implemented or security-validated. Do not wire local execution to this daemon
  until these and fresh configuration/trust checks exist. The installation journal
  has resolved-origin fields, but phase 06's preparation is not implemented; local
  startup must consume an appropriately bound immutable installation, not execute
  a floating package name from configuration.
- Next action: Implement the scoped/revision/installation-keyed instance manager
  and owned child teardown, with fresh trust/enablement checks and isolated fake
  runtime fixtures. Then extend IPC for local lifecycle and bounded MCP operations,
  wire public start/stop and passive operational status, and complete steps 2-7.
