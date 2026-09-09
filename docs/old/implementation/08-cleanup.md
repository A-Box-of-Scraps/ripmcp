# 08: Uninstall, safe cleanup and self-removal - 2026-09-08

Status: **Done**

Dependencies: 02, 04, 06, 07.

Contract gates: D09; executable provenance and confirmation details.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/cleanup/ (new or modify when introduced by an earlier phase)
+ src/ownership/ (new or modify when introduced by an earlier phase)
+ src/supervisor/ (new or modify when introduced by an earlier phase)
+ src/cli.rs (new or modify when introduced by an earlier phase)
+ tests/uninstall.rs (new or modify when introduced by an earlier phase)
+ tests/self_uninstall.rs (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Build one ownership-driven cleanup planner returning exact deletable resources, preserved resources with reasons, ordered actions and retry state. Validate containment, filesystem identity/symlinks and current ownership at deletion time; never infer ownership from a server name or path prefix alone.
2. [x] Implement ordinary `uninstall <server>`: coordinate lifecycle locks, stop only the selected owned local process, unregister in the selected scope, and preserve installed data/caches plus ownership records. Remote uninstall only changes local registration/state. Make partial stop/config failures explicit and recoverable.
3. [x] Implement `uninstall <server> --clean` and `-y`. Preview before confirmation; use [y/N]. Without a terminal fail before any mutation unless `-y` is supplied. Confirmation covers stop, unregister and deletion. `-y` changes no ownership or scope rules.
4. [x] Implement journaled best-effort deletion with reports of completed actions, failures/causes, preserved resources and exact retry instructions. Ensure retries work after registration is already gone by resolving retained cleanup records unambiguously. Preserve retry metadata until requested work is complete.
5. [x] Establish executable/install provenance before self-removal: record standalone binary identity and only genuinely installation-created links/setup. Recognize package-manager-managed or uncertain installations without guessing that the running binary is exclusively owned. Never delete PATH directories or broadly edit shell files.
6. [x] Implement `--uninstall-everything` with the same preview/confirmation contract. Acquire the maintenance barrier, stop all owned processes and supervisor, then clean tracked user configuration/data/state/log/cache/runtime resources including recorded custom locations and orphaned installation records. Remove owned saved credentials as part of cleanup.
7. [x] Always preserve project `.ripmcp` directories, external paths, shared runtimes/caches/images and uncertain resources; report these exclusions. Do not recursively search for projects. Handle a preserved project pointing to removed managed data honestly in the report.
8. [x] Remove the standalone executable last, only after earlier required cleanup succeeds. For package-managed/unknown provenance preserve the binary and report the required action and incomplete removal. Retain failure/retry state rather than erasing evidence to claim success.
9. [x] Test removal only inside temporary sandboxes using a copied test executable or injected remover. Inject permissions failures, interruption, path substitution, active calls, concurrent auto-start, duplicate ownership references and package-manager provenance.

## Acceptance criteria

- Ordinary uninstall preserves data; clean uninstall removes only tracked exclusive resources.
- Declining confirmation or lacking a terminal without `-y` makes zero lifecycle/config/filesystem mutations.
- External/shared resources and project configs survive both cleanup modes.
- Partial failures return nonzero, retain retry records and do not remove the executable prematurely.
- Cleanup cannot stop foreign processes or escape the authorized resource set through symlinks/races.
- All destructive tests are sandboxed; repository validation gate passes.

## References

server-management.md ownership and uninstall; self-uninstall.md; configuration.md storage layout; user cleanup commands.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: Steps 1-9. Ordinary uninstall, confirmed clean uninstall,
  self-removal, durable retries, and the safety regression tests are implemented.
- In-progress task: None.
- Changed paths / commits: Added `src/cleanup/`, ownership filesystem/executable/
  artifact records, cross-process maintenance admission, supervisor removal
  handlers, credential cleanup/indexing, and `tests/uninstall*` /
  `tests/self_uninstall.rs`. Updated storage, installation, trust and command
  integration plus affected regression expectations. No commits made. README,
  product documentation, changelog and lint implementation were not changed.
- Tests run and results (September 9, 2026):
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: passed; the existing opt-in real-runtime smoke test remains
    ignored. Cleanup coverage includes 15 uninstall and 9 self-uninstall tests,
    plus a fake secure-store credential-removal test.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
  - `git diff --check`: passed.
- Decisions approved: D09 remains the approved contract; no new command syntax
  or product scope was added.
- Implementation contracts:
  - Cleanup shares ownership/resource classification across removal modes.
    Deletion uses no-follow parent descriptors, device/inode and immutable birth
    identity, no-replace staging, and identity checks after staging. Directories
    must be empty after individually tracked entries are removed. Unknown/shared
    resources, untracked contents and project `.ripmcp` directories are preserved.
  - `artifacts.json` tracks newly created native configuration, trust, ownership,
    credential-index and lifecycle control files. Native file identity comes
    from the creator's open descriptor, not an uninstall-time path guess.
    Older records without sufficient identity remain conservative exclusions.
  - Ordinary uninstall keeps data and ownership records. A pending clean
    operation identifies its retry even when older same-name installations
    exist. Ambiguous orphan selection fails closed. A project retry whose
    registration is already absent needs no execution trust because it neither
    executes definitions nor modifies project configuration.
  - `self-removal.json` retains reports and journal backups until removal
    completes. The maintenance lock fences CLI mutations, supervisor requests
    and startup. Self-removal stops owned processes and the supervisor before
    deleting resources. Failed cleanup keeps the executable and retry evidence.
  - `Executable::installed_copy` and `record_created_link` are explicit
    installer attestations, not automatic claims about `current_exe`.
    Standalone removal checks recorded identity and binary SHA-256; recorded
    links are unlinked without following their targets. Package-managed and
    unknown binaries are preserved with an incomplete-removal report. PATH
    directories and unproven shell setup are never removed or broadly edited.
  - OAuth keys are indexed before secure-store mutation. Cleanup removes only
    selected ripmcp OAuth items and verifies absence. External references and
    unindexed, unproven legacy credentials are reported as exclusions. Secure
    storage failures preserve retry state and prevent executable removal.
- Blockers / remaining questions: None for phase 08. Unsupported filesystem
  identity, inaccessible secure storage and unmatched lifecycle contexts fail
  closed rather than weakening ownership checks. Destructive tests use temporary
  sandboxes and copied executables; no personal credentials or live keyring were
  used. Live provider/runtime checks remain opt-in validation work.
- Next action: Read `09-v1-validation.md` and perform the full-system validation
  phase. Product documentation remains outside these implementation phases.
