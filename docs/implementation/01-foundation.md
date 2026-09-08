# 01: Contracts, CLI foundation and test infrastructure - 2026-09-08

Status: **Done**

Dependencies: None.

Contract gates: D01-D10 and the additional contract choices in index.md.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
~ Cargo.toml
~ Cargo.lock (create if absent)
~ src/main.rs
+ src/lib.rs (new or modify when introduced by an earlier phase)
+ src/cli.rs (new or modify when introduced by an earlier phase)
+ src/error.rs (new or modify when introduced by an earlier phase)
+ src/output.rs (new or modify when introduced by an earlier phase)
+ tests/cli.rs (new or modify when introduced by an earlier phase)
+ tests/support/ (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Resolve or explicitly block the decision register. Preserve the user's command surface; approve additional install/scope/timeout syntax. Record choices in the tracker before coding affected behavior.
2. [x] Select minimal Rust dependencies against the approved profile. Verify protocol-library support from official sources at implementation time; do not select a library based only on its latest release label. Keep protocol and process abstractions testable independently.
3. [x] Replace the entry-point stub with parser/dispatcher modules and typed command requests for every listed command. Validate mutually exclusive flags, server/tool arity, `--input`, `-y`, and self-uninstall conflicts without side effects. Unsupported handlers must return explicit errors, never success stubs.
4. [x] Define exact parsing rules for qualified and unqualified call forms, including inline JSON and `--input`. Parse positionals without guessing based on tool discovery. Reject missing, duplicate, malformed, or unexpected arguments consistently.
5. [x] Implement typed errors, stable exit codes, structured stdout writers, and stderr-only diagnostics/prompts. Define connection/protocol/auth/tool-result/partial-failure distinctions, timeout behavior and secret redaction.
6. [x] Build isolated test utilities: CLI process runner, temporary HOME/XDG environment, fake runtime commands, stdio MCP server, local HTTP/OAuth fixtures, and fault injection. Add parser/output tests now; implement protocol-specific fixture behavior in phase 03.

## Acceptance criteria

- Every user-listed form has parser coverage, including all six call input/qualification combinations.
- Parser failures produce nonzero exits and cannot create config, launch processes, or read credentials.
- Data output and diagnostics are separated; no handler claims an unimplemented operation succeeded.
- Blocking choices have approved decisions or explicit blocked dependencies; the repository validation gate passes.

## References

All idea files; commands.md; contracts.md; AGENTS.md; Cargo.toml; src/main.rs.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed: all six foundation tasks. The authoritative user command list is
  saved in `commands.md`; the parser includes `trust` and every listed form.
  D01-D10 approval and additional engineering contracts are recorded. No command
  list or foundation contract questions remain.
- Implemented: typed CLI requests; side-effect-free call grammar and conflict
  validation; stable error codes; complete JSON/tool-result output with failure
  status; strict timeout config/defaults and monotonic deadline primitive;
  isolated CLI runner, fake runtime/stdio scripts, loopback HTTP/OAuth transport
  scaffold and malformed/closure/stall fault injection.
- Coverage: all six call forms through parsing and the CLI process; all listed
  commands; trust/scope/self-uninstall conflicts; stdout/stderr separation;
  no parser filesystem effects or secret echoes; timeout precedence/config
  validation/overflow safety; tool-result preservation; fixture cleanup/failures.
- Changed paths in this completion: `src/{cli,lib,deadline,output}.rs`,
  `tests/{cli,contracts,fixtures}.rs`, `tests/support/fixtures/{http,process}.rs`,
  and `docs/implementation/{index,01-foundation,commands,contracts}.md`.
  No README, lint implementation or dependency changes. No commits made.
- Final validation on September 8, 2026: all five required commands passed:
  `cargo fmt -q`; `cargo clippy -q --all-targets -- -D warnings`;
  `cargo test -q`; `cargo dylint --all -- --locked --all-targets`;
  `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`.
  Application tests: 18 (six CLI, six contract, six fixture tests, with case
  matrices). Lint tests: one harness test with two passing UI cases. No
  environmental limitations. `git diff --check` also passed.
- Fixed during validation: root-level timeout/subcommand conflict and one missing
  explicit array type in the HTTP fixture. Full gate passed after both fixes.
- Deliberate boundaries: operational handlers, including trust and shape, return
  explicit unsupported errors until their phases. Fixture scaffolding is not
  protocol compatibility evidence. Phase 03 adds full protocol behavior,
  asynchronous deadline enforcement and signal cancellation; phase 05 adds OAuth
  validation and secure storage. No protocol library was selected prematurely.
- Downstream gates: D05 concrete SDK revision/transport audit, D08 authorization
  and secure-store audit, D10 runtime-directory/socket security tests. Official
  source inspection and observed SDK default-version gap are in `contracts.md`.
  These gates block affected integrations, not independent configuration work.
- Next executable task: phase 02 step 1, implement lazy XDG path resolution and
  security tests against the recorded contracts. Do not start phase 03 protocol
  integration until its dependencies and compatibility audit are complete.

## Contract references

- `commands.md`: confirmed command surface and exact install/scope/input syntax.
- `contracts.md`: timeout schema and precedence, exit codes/output/redaction,
  shape bounds, pinned runtime resolution, verification retention, remote client
  ownership, credential-reference syntax, passive untrusted reads, trust prompts,
  official-source inspection and fixture limits.
- `index.md`: user-approved D01-D10, architecture and overall phase status.
