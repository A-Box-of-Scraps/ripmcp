# 09: Full-system validation and code-complete handoff - 2026-09-08

Status: **Blocked**

Dependencies: 01-08.

Contract gates: All decision-register entries resolved; no unimplemented public handlers.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ tests/ (new or modify when introduced by an earlier phase)
+ affected src/ modules from earlier phases (new or modify when introduced by an earlier phase)
+ Cargo.toml/Cargo.lock if required for fixes (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Build a command-coverage matrix in tests for every user-listed command/form, scope and relevant local/remote behavior. Confirm no placeholder handlers or hidden success paths remain.
2. [x] Exercise full local lifecycle: install/verify, list, disable/enable tool, qualified/shorthand calls across separate CLI processes, stop/auto-start, server disable, ordinary uninstall, reinstall and owned-data cleanup. Include fake npx/uvx/Docker adapters and opt-in real-runtime smoke tests.
3. [ ] Exercise full remote lifecycle: register, explicit blocking login, status, discovery/call, expiry/refresh, logout, auth-required call, endpoint override/trust renewal and uninstall. Verify remote start/stop remains rejected.
4. [x] Exercise cross-cutting failures: unavailable server during shorthand lookup, concurrent config writes, revoked project trust with a running supervisor, crash during install/cleanup, PID/socket reuse, cancellation, disk full/permission failures and unavailable secure storage.
5. [ ] Audit side-effect boundaries and secrets: no tool replay, no unintended auto-start, no credentials in stdout/logs/project configs, no project data deletion or external resource deletion. Test large result memory/error behavior and shape bounds without silent truncation.
6. [x] Run the complete repository validation gate from a clean isolated test environment. Run a release build and CLI smoke test. Record precise toolchain/runtime/platform constraints and opt-in test results without presenting unrun checks as successful.
7. [ ] Fix regressions in the owning subsystem, update its phase evidence, then rerun affected and full tests. Confirm all phase acceptance criteria and approved decisions are reflected in the code.
8. [x] Mark implementation complete only when required gates pass, or explicitly leave the tracker blocked with remaining failures. Record the code-complete handoff here. Do not create product documentation; hand that final step back to the user.

## Acceptance criteria

- Every public command is implemented and covered by success, validation and relevant failure tests.
- Protocol/auth profile is explicit and verified; no unresolved scope/contract blockers remain.
- Persistent local behavior, trust isolation, unique-name safety and cleanup ownership survive fault tests.
- Required validation commands pass; environmental gaps and optional smoke-test omissions are clearly listed.
- Tracker accurately distinguishes completed work from deferred scope and any remaining blockers.
- No README, tutorials, changelog prose or agent-skill deliverables were added as implementation work.

## References

All docs/ideas/*.md; user command list; AGENTS.md; phase 01 approved contracts; phase 02-08 acceptance criteria.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (September 9, 2026)

- Completed tasks: Command/form/scope grammar matrix and handler evidence map in
  `tests/validation/coverage.{rs,md}`. Exhaustive public dispatch now has no
  placeholder fallback. Added combined installed local lifecycle tests for all
  three fake runtime adapters: verification without calls, policy, process reuse,
  stop/auto-start, disabled execution, ordinary uninstall, reinstall, retained old
  data and confirmed exclusively owned new-data cleanup. Added unauthenticated
  remote registration/discovery/call/uninstall with explicit start/stop rejection
  and request counts. Existing fault suites plus new actual ENOSPC and `/dev/full`
  tests cover cross-cutting failures. Required repository gates, opt-in disk-full
  test and release build/smoke pass after the queue-test correction below.
- Partial tasks: Step 3's OAuth lifecycle is tested through separate provider,
  callback, fake secure-store, transport and CLI seams, not one authenticated
  end-to-end CLI lifecycle. Step 5 has full-envelope, transport-limit, no-replay,
  shape-bound and secret/ownership tests, but no measured maximum-size concurrent
  reply memory stress evidence. Step 7's discovered test race is fixed and
  rerun; all acceptance criteria cannot yet be confirmed because of those gaps.
- Changed paths / commits: `src/lib.rs`; `tests/lifecycle/ipc.rs`;
  `tests/validation.rs`, `tests/validation/{coverage.rs,coverage.md,local.rs,
  remote.rs,failures.rs}`; phase 01/04 evidence and phase 09/overview tracker.
  No commits made. No README, product documentation, changelog or skills changed.
- Regression found and fixed: The initial clean isolated `cargo test -q` failed
  `ipc::per_server_queue_is_bounded_and_disconnect_releases_queued_work`. A probe
  could take the last queue slot and reject a worker, then miss saturation. The
  test now submits 17 stalled workers for the 16-slot limit, observes rejection,
  cancels/drains the others and checks server reuse. Production limits were not
  changed. The corrected test passed 20 consecutive targeted runs and the full
  suite. Phase 04 records this correction.
- Tests run and results:
  - Baseline `cargo test -q`: passed before edits.
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: 254 passed, 2 ignored, 0 failed on the corrected full run.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including the UI test harness.
  - `jscpd .`: passed below the configured 4 percent threshold.
  - `cargo test -q --test validation failures::disk_full -- --ignored`: passed.
    This mounts a private 64 KiB tmpfs in new user/mount namespaces, observes
    actual ENOSPC (errno 28), verifies old committed storage/config bytes after
    failure, releases space and successfully retries. The host filesystem is
    never filled or mounted over outside that namespace.
  - `cargo build -q --release --locked`: passed. Isolated release `--version`,
    `--help`, empty `servers`, and `shape -` smoke checks passed.
  - Real npx/uvx/Docker registry/daemon smoke: NOT RUN. The ignored smoke requires
    explicitly selected `RIPMCP_SMOKE_RUNTIME` and `RIPMCP_SMOKE_PACKAGE` values.
    No real package, public MCP provider or personal credentials were used.
- Environment: Arch Linux, Linux `7.2.3-arch1-3`, x86_64 GNU; rustc `1.97.1
  (8bab26f4f 2026-07-14)`, cargo `1.97.1 (c980f4866 2026-06-30)`, stable
  `x86_64-unknown-linux-gnu`; Clang `22.1.8` with the repository LLD configuration;
  Python `3.14.7`; jscpd/cpd `5.0.14`. Linux pidfds, private Unix sockets and
  unprivileged user/mount namespaces were available. The disk-full opt-in also
  requires `unshare`, `sh`, and `mount`; default tests do not require namespaces.
- Isolation/reproduction: Gate commands ran under `env -i` with fresh private
  HOME/XDG directories, unset session bus/browser/credential variables, explicit
  PATH/CARGO_HOME/RUSTUP_HOME, `CARGO_NET_OFFLINE=true`, and an initially fresh
  `CARGO_TARGET_DIR=$PWD/target/phase9-isolated`. Dependency/toolchain caches were
  reused, not personal application configuration. The runner and command logs
  are at `/tmp/ripmcp-phase9-gate.J8oqI8/`; `results-first-run` and
  `test-first-run.log` retain the original failure. These temporary logs are not
  required repository artifacts; committed tests and commands reproduce checks.
- Decisions approved: No new scope or protocol/auth decisions. D01-D10 and prior
  phase contracts remain unchanged. The prior pinned protocol/auth audit is the
  compatibility profile; this phase does not claim new live-provider validation.
- Blockers / remaining work: Add an isolated authenticated full CLI lifecycle
  fixture that exercises production secure-store/TLS integration, blocking login,
  discovery/call, expiry/refresh, logout, auth-required call, and endpoint override
  with trust renewal. Do not weaken production HTTPS/private-network restrictions
  or add a plaintext credential fallback for the fixture. Add peak-memory/error
  stress evidence for large concurrent supervisor results. Optional real-runtime
  omissions are recorded above and are not themselves required completion gates.
- Next action: Implement that authenticated integration fixture, add memory stress
  coverage, then rerun affected tests and the complete gate. Only then mark phase
  09/code-complete and hand product documentation back to the user. V1 is NOT
  declared code-complete by this handoff.
