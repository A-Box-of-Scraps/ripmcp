# 01: Contracts, CLI foundation and test infrastructure - 2026-09-08

Status: **Not started**

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

1. [ ] Resolve or explicitly block the decision register. Preserve the user's command surface; approve additional install/scope/timeout syntax. Record choices in the tracker before coding affected behavior.
2. [ ] Select minimal Rust dependencies against the approved profile. Verify protocol-library support from official sources at implementation time; do not select a library based only on its latest release label. Keep protocol and process abstractions testable independently.
3. [ ] Replace the entry-point stub with parser/dispatcher modules and typed command requests for every listed command. Validate mutually exclusive flags, server/tool arity, `--input`, `-y`, and self-uninstall conflicts without side effects. Unsupported handlers must return explicit errors, never success stubs.
4. [ ] Define exact parsing rules for qualified and unqualified call forms, including inline JSON and `--input`. Parse positionals without guessing based on tool discovery. Reject missing, duplicate, malformed, or unexpected arguments consistently.
5. [ ] Implement typed errors, stable exit codes, structured stdout writers, and stderr-only diagnostics/prompts. Define connection/protocol/auth/tool-result/partial-failure distinctions, timeout behavior and secret redaction.
6. [ ] Build isolated test utilities: CLI process runner, temporary HOME/XDG environment, fake runtime commands, stdio MCP server, local HTTP/OAuth fixtures, and fault injection. Add parser/output tests now; implement protocol-specific fixture behavior in phase 03.

## Acceptance criteria

- Every user-listed form has parser coverage, including all six call input/qualification combinations.
- Parser failures produce nonzero exits and cannot create config, launch processes, or read credentials.
- Data output and diagnostics are separated; no handler claims an unimplemented operation succeeded.
- Blocking choices have approved decisions or explicit blocked dependencies; the repository validation gate passes.

## References

All idea files; user command list; AGENTS.md; Cargo.toml; src/main.rs.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Review and approve the contract decisions, then implement CLI and test foundations.
