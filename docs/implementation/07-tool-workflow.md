# 07: Enablement, discovery, invocation and offline shape - 2026-09-08

Status: **Done**

Dependencies: 03, 04, 05, 06.

Contract gates: D03, D06-D07; shape limits and listing freshness.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/tools/ (new or modify when introduced by an earlier phase)
+ src/shape/ (new or modify when introduced by an earlier phase)
+ src/output.rs (new or modify when introduced by an earlier phase)
+ src/cli.rs (new or modify when introduced by an earlier phase)
+ tests/tools.rs (new or modify when introduced by an earlier phase)
+ tests/call.rs (new or modify when introduced by an earlier phase)
+ tests/shape.rs (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Implement persisted server/tool enable and disable operations in the chosen scope. Tool policy is default allow: retain disabled names across discovery changes/restarts and enable new tools automatically. Server disable takes precedence. Define errors for unknown server/tool targets.
2. [x] Enforce effective policy before discovery/dispatch and again at the execution boundary where needed. Disabled direct calls fail without implicit enabling; define policy-change races with already-dispatched calls. Disable does not revoke or cancel a tool call already executed.
3. [x] Implement `tools`, `tools <server>`, `tools <server> --all`, and `tool <server> <tool>`. Keep full schemas in detail output only. Include disabled state where requested. Expose per-server discovery failures and incomplete results with a nonzero status rather than hiding outages.
4. [x] Implement complete discovery freshness rules for shorthand resolution. Search only enabled servers/tools; exactly one match permits invocation only when uniqueness is established across all eligible servers. On collision list qualified candidates, on no match fail, and on any incomplete discovery refuse to infer uniqueness. Qualified calls never require unrelated server discovery.
5. [x] Implement all inline/file/stdin call forms with exactly one JSON argument source, approved root/schema validation behavior and useful read/parse errors. Never echo secret input in diagnostics. Ensure invalid input fails before a tool invocation and local qualified calls auto-start only when enabled/trusted.
6. [x] Write the full MCP result envelope once to stdout. Preserve text, structured and non-text content; preserve tool-error payloads while returning the selected nonzero code. Handle broken pipes and serialization failures without retrying the call. Do not introduce automatic result saving or a preview invocation flag.
7. [x] Implement built-in `shape <file>` and `shape -` over saved JSON, with field/type/array-length output and no scalar values. Define heterogeneous-array representation and explicit depth/width/size limits; report omitted structure. Shape must not load configuration, start the supervisor, access auth or invoke any server.
8. [x] Add call-once/save/shape regression tests, persistence tests and exact machine-output/exit-code assertions for all public tool commands.

## Acceptance criteria

- Disabled servers/tools cannot be invoked by qualified or shorthand calls; new tools remain enabled by default.
- Shorthand ambiguity and incomplete discovery invoke zero tools; qualification bypasses unrelated outages.
- Repeated local calls reuse the process; each successful call request dispatches once only.
- Large results remain complete; saved results can be shaped offline with no additional tool call.
- Stdout stays machine-readable even on partial discovery/tool failures; stderr carries diagnostics.
- File/stdin malformed input, Unicode, deep/wide structures and heterogeneous arrays have tests.
- Repository validation gate passes.

## References

tool-workflow.md; server-management.md enablement; index.md unqualified resolution and shape agreements; user command list.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (September 8, 2026)

- Completed: all eight steps. Persisted server/tool policy, concise discovery,
  complete cross-server resolution, all argument sources, complete result output,
  and offline bounded shape are implemented. Existing qualified local/remote
  transport and call-once output paths are reused.
- Policy: unknown scoped servers exit 3; unknown tools exit 2. Retained disabled
  names can be edited without rediscovery, including while the server is disabled.
  Otherwise tool validation requires enabled, trusted discovery of the selected
  scoped definition. A shadowed user tool needing discovery fails with an explicit
  instruction to run outside the overriding project rather than inspecting the
  wrong server. Mutations report trusted shadowing and project reapproval.
  Semantic no-ops preserve exact configuration bytes and existing trust.
- Discovery: list entries contain server/name/enabled and optional title/description;
  only detail output includes schemas. Lists always include an errors array.
  Unavailable servers and rejected definitions make cross-server discovery exit 8;
  partial valid entries remain on stdout. Shorthand collisions exit 2 with qualified
  candidates. Each resolution performs fresh discovery and detects configuration
  changes across the discovery interval. Qualified calls bypass unrelated servers.
- Execution: local and remote paths recheck policy before dispatch. A policy edit
  cannot retract a request already transmitted; disable does not stop a process or
  cancel that call. Calls and stdout writes are never retried.
- Input/shape: strict, lossless JSON object call inputs, with server-side general
  schema validation and existing client-side HTTP annotation validation. Shape
  permits any JSON root and never loads configuration or resolves credentials.
  Arrays expose per-index shapes, preserving heterogeneous structure without
  scalar values. Limits and omitted counts are described in contracts.md.
- Changed paths: src/tools/{mod,policy}.rs, src/shape.rs, CLI/dispatch,
  supervisor command/input/list reporting, remote list/dispatch checks, config
  equality/no-op mutation support, cloneable supervisor request types, tests/tools.rs,
  tests/shape.rs, tests/{auth,cli}.rs, lifecycle fixture, and implementation tracker.
  No commits made; README and product documentation are unchanged.
- Regression coverage: default allow and disappeared/reappearing tools; policy
  persistence and scoped trust; incomplete remote annotations/outages; ambiguity;
  all six call forms; invalid input redaction; process reuse; call-once/save/shape;
  multi-megabyte tool-error results; broken pipes; edits before/after dispatch;
  Unicode, heterogeneous arrays, depth/width/node limits, oversized/missing files,
  and offline shape with missing HOME and malformed project configuration.
- Validation: all repository gates passed: `cargo fmt -q`,
  `cargo clippy -q --all-targets -- -D warnings`, `cargo test -q`,
  `cargo dylint --all -- --locked --all-targets`, and
  `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`.
  The existing opt-in real-runtime installation smoke test remains ignored.
  `git diff --check` also passed. Initial failures were obsolete unsupported-handler
  assertions and an incorrectly constructed test annotation; both were corrected.
- Blockers: none. No public servers, personal credentials, or real package/runtime
  installations were used for validation.
- Next action: read 08-cleanup.md and inspect supervisor maintenance/ownership
  contracts before implementing cleanup.
