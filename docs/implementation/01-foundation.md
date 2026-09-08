# 01: Contracts, CLI foundation and test infrastructure - 2026-09-08

Status: **Blocked** (foundation implemented; authoritative command list and remaining contracts needed)

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
4. [x] Define exact parsing rules for qualified and unqualified call forms, including inline JSON and `--input`. Parse positionals without guessing based on tool discovery. Reject missing, duplicate, malformed, or unexpected arguments consistently.
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

- Completed: approved D01-D10; typed CLI for recoverable command forms; all six
  call forms; side-effect-free argument validation; stable error kinds/codes;
  JSON output writer; redacted stderr diagnostics; isolated HOME/XDG CLI runner.
- Partial tasks: 01-03 and 05-06. Timeout execution/cancellation, fake runtimes,
  stdio and HTTP/OAuth fixture scaffolding, and remaining contracts are not done.
  All operational handlers explicitly fail with exit 10; none claim success.
- Changed paths: Cargo.toml, Cargo.lock, src/{main,lib,cli,call,error,output}.rs,
  tests/cli.rs, tests/support/mod.rs, and both phase tracker files. No commits.
- Validation: `cargo fmt -q`, `cargo clippy -q --all-targets -- -D warnings`,
  `cargo test -q`, `cargo dylint --all -- --locked --all-targets`, and
  `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`
  all passed. Application suite: six tests with command/input case matrices.
  Lint suite: one harness test covering two UI cases. Initial parser conflict
  and explicit-local-type failures were fixed, then the full gate rerun.
- Decisions approved: D01-D10, preserving their technical verification gates.
- Blockers: authoritative user command list unavailable; remaining additional
  choices below need settlement. Protocol SDK coverage is not yet verified.
- Next action: obtain the authoritative command list (especially trust and
  self-uninstall spellings), settle remaining contracts, complete fixture
  scaffolding and coverage, then rerun the gate before starting phase 02.

## Foundation contracts recorded before implementation

- D01-D10 are user-approved. The original authoritative command-list message is
  not present in this session or the tracker. Implement the forms recoverable
  from the idea files; exact command-surface acceptance remains blocked pending
  that list, especially trust and self-uninstall spellings.
- Call grammar: without `--input`, exactly `tool JSON` or `server tool JSON`;
  with `--input`, exactly `tool` or `server tool`. JSON must be an object.
  File/stdin input is not read during parsing. No discovery-based parsing.
- Foundation defaults: global `--timeout <seconds>` is a positive integer,
  default 60; login timeout is 300 seconds unless explicitly overridden.
  Cancellation is exit 130; timed-out work is exit 9; never retry calls implicitly.
- Exit codes: 0 success, 1 internal/I/O, 2 usage/input, 3 configuration/trust,
  4 connection, 5 protocol, 6 authentication, 7 tool-result failure,
  8 partial failure, 9 timeout, 10 unsupported, 130 cancellation.
- JSON writers append one newline and preserve envelopes; diagnostics never
  include raw arguments, JSON values, credentials, URLs, or underlying errors.
- Shape parser defaults: depth 8 (maximum 64), width 100 (maximum 10000).
  Shape execution and truncation indicators belong to phase 07.
- Remaining implementation choices (not silently approved): timeout config
  schema, runtime version resolution, verification retention, remote session
  ownership, credential-reference format, and untrusted diagnostic reads.
- Dependencies are clap derive, serde, serde_json, and test-only tempfile.
  Official specification at `https://modelcontextprotocol.io/specification/2026-07-28`
  was accessible on September 8, 2026. Official Rust SDK at
  `https://github.com/modelcontextprotocol/rust-sdk` was inspected, but compatible
  revision/authorization coverage is not established. No protocol dependency is
  selected; D05 remains a phase 03 verification gate, not a revision substitution.
