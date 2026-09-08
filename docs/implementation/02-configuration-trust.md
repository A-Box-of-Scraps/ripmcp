# 02: Configuration, storage, trust and ownership foundations - 2026-09-08

Status: **Not started**

Dependencies: 01.

Contract gates: D01-D03, D09-D10; secret-reference format.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/config/ (new or modify when introduced by an earlier phase)
+ src/storage/ (new or modify when introduced by an earlier phase)
+ src/trust/ (new or modify when introduced by an earlier phase)
+ src/ownership/ (new or modify when introduced by an earlier phase)
+ tests/config.rs (new or modify when introduced by an earlier phase)
+ tests/trust.rs (new or modify when introduced by an earlier phase)
+ tests/storage.rs (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [ ] Implement lazy XDG path resolution and validation with agreed defaults for config, data, state, cache and runtime. Enforce private permissions for sensitive state. Validate runtime fallback ownership, directory type and symlinks.
2. [ ] Define versioned user/project schemas, typed local/remote definitions, server enablement and per-server disabled-tool sets. Reject unsupported schema versions and invalid transport/runtime combinations with useful field errors.
3. [ ] Implement project discovery, whole-definition override rules and explicit write scopes. Keep provenance and canonical identity in the effective configuration. Use locked, atomic read-modify-write operations to prevent concurrent mutation loss.
4. [ ] Implement `trust`: approve the exact configuration content and canonical project root, save approvals outside the repository, and require reapproval after content changes. A prompt must preview the relevant execution/endpoint effects. Define safe noninteractive trust behavior explicitly.
5. [ ] Enforce trust before loading project secret references, provisioning, execution, discovery, authentication or forwarding a supervisor request. Do not silently run a global definition hidden by an untrusted project override. Bind approval to the bytes actually parsed to avoid check/use races.
6. [ ] Add ownership and operation-journal types before any installation: immutable installation ID, scope, resource kind, canonical path or runtime identity, origin, exclusive/shared/unknown ownership, retained-data references and cleanup state. Track custom locations without recursive filesystem scanning.
7. [ ] Implement secret-reference resolution boundaries and redacted display models. Keep generated state and credentials outside project configs. Add recovery tests for interrupted writes and incompatible/corrupt state.

## Acceptance criteria

- Nested discovery, same-name overrides, explicit scope writes, malformed config and concurrent writers have deterministic tests.
- Content changes, moved projects, symlink paths and endpoint changes cannot reuse stale trust.
- Read-only operations do not create unnecessary directories or launch servers.
- Ownership records can survive ordinary unregistering and support later cleanup retries.
- HOME/XDG and filesystem safety tests pass with the repository validation gate.

## References

configuration.md; server-management.md ownership agreement; authentication.md credential binding; self-uninstall.md.

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
