# 04: Persistent supervisor, local lifecycle and status - 2026-09-08

Status: **Not started**

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

1. [ ] Implement a lazily started per-user supervisor and versioned IPC protocol over a private Unix socket. Handle concurrent bootstrap with locking, socket ownership checks and peer authentication. Reject incompatible clients and unsafe/stale socket reuse.
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

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
