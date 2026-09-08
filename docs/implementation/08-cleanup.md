# 08: Uninstall, safe cleanup and self-removal - 2026-09-08

Status: **Not started**

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

1. [ ] Build one ownership-driven cleanup planner returning exact deletable resources, preserved resources with reasons, ordered actions and retry state. Validate containment, filesystem identity/symlinks and current ownership at deletion time; never infer ownership from a server name or path prefix alone.
2. [ ] Implement ordinary `uninstall <server>`: coordinate lifecycle locks, stop only the selected owned local process, unregister in the selected scope, and preserve installed data/caches plus ownership records. Remote uninstall only changes local registration/state. Make partial stop/config failures explicit and recoverable.
3. [ ] Implement `uninstall <server> --clean` and `-y`. Preview before confirmation; use [y/N]. Without a terminal fail before any mutation unless `-y` is supplied. Confirmation covers stop, unregister and deletion. `-y` changes no ownership or scope rules.
4. [ ] Implement journaled best-effort deletion with reports of completed actions, failures/causes, preserved resources and exact retry instructions. Ensure retries work after registration is already gone by resolving retained cleanup records unambiguously. Preserve retry metadata until requested work is complete.
5. [ ] Establish executable/install provenance before self-removal: record standalone binary identity and only genuinely installation-created links/setup. Recognize package-manager-managed or uncertain installations without guessing that the running binary is exclusively owned. Never delete PATH directories or broadly edit shell files.
6. [ ] Implement `--uninstall-everything` with the same preview/confirmation contract. Acquire the maintenance barrier, stop all owned processes and supervisor, then clean tracked user configuration/data/state/log/cache/runtime resources including recorded custom locations and orphaned installation records. Remove owned saved credentials as part of cleanup.
7. [ ] Always preserve project `.ripmcp` directories, external paths, shared runtimes/caches/images and uncertain resources; report these exclusions. Do not recursively search for projects. Handle a preserved project pointing to removed managed data honestly in the report.
8. [ ] Remove the standalone executable last, only after earlier required cleanup succeeds. For package-managed/unknown provenance preserve the binary and report the required action and incomplete removal. Retain failure/retry state rather than erasing evidence to claim success.
9. [ ] Test removal only inside temporary sandboxes using a copied test executable or injected remover. Inject permissions failures, interruption, path substitution, active calls, concurrent auto-start, duplicate ownership references and package-manager provenance.

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

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
