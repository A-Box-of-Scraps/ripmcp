# 09: Full-system validation and code-complete handoff - 2026-09-08

Status: **Not started**

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

1. [ ] Build a command-coverage matrix in tests for every user-listed command/form, scope and relevant local/remote behavior. Confirm no placeholder handlers or hidden success paths remain.
2. [ ] Exercise full local lifecycle: install/verify, list, disable/enable tool, qualified/shorthand calls across separate CLI processes, stop/auto-start, server disable, ordinary uninstall, reinstall and owned-data cleanup. Include fake npx/uvx/Docker adapters and opt-in real-runtime smoke tests.
3. [ ] Exercise full remote lifecycle: register, explicit blocking login, status, discovery/call, expiry/refresh, logout, auth-required call, endpoint override/trust renewal and uninstall. Verify remote start/stop remains rejected.
4. [ ] Exercise cross-cutting failures: unavailable server during shorthand lookup, concurrent config writes, revoked project trust with a running supervisor, crash during install/cleanup, PID/socket reuse, cancellation, disk full/permission failures and unavailable secure storage.
5. [ ] Audit side-effect boundaries and secrets: no tool replay, no unintended auto-start, no credentials in stdout/logs/project configs, no project data deletion or external resource deletion. Test large result memory/error behavior and shape bounds without silent truncation.
6. [ ] Run the complete repository validation gate from a clean isolated test environment. Run a release build and CLI smoke test. Record precise toolchain/runtime/platform constraints and opt-in test results without presenting unrun checks as successful.
7. [ ] Fix regressions in the owning subsystem, update its phase evidence, then rerun affected and full tests. Confirm all phase acceptance criteria and approved decisions are reflected in the code.
8. [ ] Mark implementation complete only when required gates pass, or explicitly leave the tracker blocked with remaining failures. Record the code-complete handoff here. Do not create product documentation; hand that final step back to the user.

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

## Handoff (update whenever work stops)

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
