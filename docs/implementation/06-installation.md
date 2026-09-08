# 06: Local installation and remote registration transactions - 2026-09-08

Status: **Not started**

Dependencies: 02, 03, 04, 05.

Contract gates: D03-D05; version resolution and verification process retention.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/install/ (new or modify when introduced by an earlier phase)
+ src/ownership/ (new or modify when introduced by an earlier phase)
+ src/config/ (new or modify when introduced by an earlier phase)
+ tests/install.rs (new or modify when introduced by an earlier phase)
+ tests/support/ (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [ ] Implement the approved named install forms and native JSON import with strict validation. Convert all forms into one typed installation request; parse runtime arguments as argument vectors, not concatenated shell commands. Require trust before processing executable project definitions.
2. [ ] Implement npx, uvx and Docker runtime adapters. Check prerequisites, prepare the package/image and capture the exact launch configuration plus resolved version/identity where supported. Respect the approved pinning policy and fail explicitly when a requested version cannot be honored.
3. [ ] Allocate dedicated managed locations lazily and journal exclusively owned resources as they are created. Treat shared package caches/images, existing volumes, user paths and bind mounts as preserved resources regardless of server usage.
4. [ ] Implement install as a locked staged transaction: validate input and target, prepare resources, initialize/connect, discover tools, then atomically commit the definition and usable state. Do not invoke a tool for verification. Preserve an existing definition on all failed attempts.
5. [ ] Implement `--skip-verify` so preparation still happens and the committed server is explicitly unverified. Remote registration skips connection/discovery only; it does not bypass config, scope, trust or ownership validation.
6. [ ] Integrate explicit authentication-required failures into remote verification. Ensure the error gives a usable login/retry path even before a server definition is committed; decide whether to retain a non-active pending registration or direct the user through skip-verify registration, login and later discovery.
7. [ ] On failure/cancellation roll back owned provisional processes/resources when safe, report residual resources, and retain recovery records for incomplete rollback. Ordinary rollback must not delete shared caches or overwrite concurrent config changes.
8. [ ] Return installation results identifying scope, runtime/endpoint, verification state and preserved/prepared resources without secrets. Apply the approved policy for retaining or stopping the process used for successful verification.

## Acceptance criteria

- Fake-runtime tests cover successful npx/uvx/Docker preparation, missing runtimes, bad versions, runtime failure and cancellation.
- Verification connects and discovers but makes zero tool calls; skip-verify still prepares dependencies.
- Remote registration/authentication failures provide a workable recovery path.
- Failed installs do not replace existing definitions or leave falsely successful registration state.
- Interrupted commits and rollback failures retain sufficient ownership/retry records.
- Repository validation gate passes; real-runtime smoke tests remain opt-in.

## References

server-management.md installation; configuration.md; authentication.md; user install commands.

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
