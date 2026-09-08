# 02: Configuration, storage, trust and ownership foundations - 2026-09-08

Status: **Done**

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

1. [x] Implement lazy XDG path resolution and validation with agreed defaults for config, data, state, cache and runtime. Enforce private permissions for sensitive state. Validate runtime fallback ownership, directory type and symlinks.
2. [x] Define versioned user/project schemas, typed local/remote definitions, server enablement and per-server disabled-tool sets. Reject unsupported schema versions and invalid transport/runtime combinations with useful field errors.
3. [x] Implement project discovery, whole-definition override rules and explicit write scopes. Keep provenance and canonical identity in the effective configuration. Use locked, atomic read-modify-write operations to prevent concurrent mutation loss.
4. [x] Implement `trust`: approve the exact configuration content and canonical project root, save approvals outside the repository, and require reapproval after content changes. A prompt must preview the relevant execution/endpoint effects. Define safe noninteractive trust behavior explicitly.
5. [x] Enforce trust before loading project secret references, provisioning, execution, discovery, authentication or forwarding a supervisor request. Do not silently run a global definition hidden by an untrusted project override. Bind approval to the bytes actually parsed to avoid check/use races.
6. [x] Add ownership and operation-journal types before any installation: immutable installation ID, scope, resource kind, canonical path or runtime identity, origin, exclusive/shared/unknown ownership, retained-data references and cleanup state. Track custom locations without recursive filesystem scanning.
7. [x] Implement secret-reference resolution boundaries and redacted display models. Keep generated state and credentials outside project configs. Add recovery tests for interrupted writes and incompatible/corrupt state.

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

Completed September 8, 2026.

- Completed tasks: All seven steps. Added lazy XDG resolution, descriptor-relative
  no-follow storage, deadline-bounded advisory locks and atomic writes; versioned
  schemas, canonical discovery, whole-definition overrides and scoped mutation
  APIs; immutable parsed snapshots, external content-bound trust, redacted previews
  and gated secret resolution; durable ownership and operation-journal types.
- Operational CLI: `servers` is passive and redacted. `trust` requires terminal
  stdin/stderr, previews before writing, defaults to no, and approves only the
  previewed canonical root/content digest. Its mutation deadline starts after
  confirmation and honors `--timeout` and trusted effective settings.
- In-progress task: None.
- Changed paths: `Cargo.toml`, `Cargo.lock`, `src/config/`, `src/storage/`,
  `src/trust/`, `src/ownership/`, `src/lib.rs`, `src/error.rs`, `src/deadline.rs`,
  `tests/config.rs`, `tests/config/`, `tests/cli.rs`, and implementation trackers/
  contracts. README, product documentation, changelog and lint implementation are
  unchanged. No commits made.
- Tests run and results:
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q --test config`: passed, 34 tests.
  - `cargo test -q`: passed, 53 tests total, including 35 new phase 02 tests.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI cases.
  - `git diff --check`: passed.
  - An intermediate full run encountered `ExecutableFileBusy` in the unchanged
    process fixture. Subsequent full runs passed without changing that fixture.
- Security evidence: sandboxed fallback/mode/type/symlink checks; foreign-owner
  permission validation; FIFO/hardlink and unsafe lock/state rejection; concurrent
  directory creation and user-config writers; cross-process lock serialization;
  deadline-limited lock waits; interrupted pending-write recovery; corrupt and
  incompatible state preservation; moved/changed/aliased project identity tests;
  no shadowed-user fallback; secret-backend call counting; real pseudo-terminal
  approval/default-no and preview-to-write race tests. Passive reads create no
  storage or server processes.
- Decisions: Follow D01-D03, D09-D10 and the secret-reference contract. Concrete
  schema/storage choices are recorded in `contracts.md`. D10's directory/locking
  gate passed; this does not approve unverified socket behavior.
- Blockers / remaining questions: None for phase 02. Keyring/OAuth backend and
  credential binding remain phase 05 work. Provisioning, protocol, authentication
  and supervisor handlers remain unsupported, not simulated.
- Next action: Begin phase 03's concrete protocol/library audit. Each future
  execution/authentication/discovery/supervisor request must load a fresh
  `Effective`, obtain its `AuthorizedServer`, check enablement/tool policy, and
  use that snapshot rather than rereading unverified project bytes. Propagate the
  operation's existing deadline through storage mutation APIs. Wire the scoped
  mutation and journal APIs into later installation/policy/cleanup handlers.
