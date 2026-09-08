# 07: Enablement, discovery, invocation and offline shape - 2026-09-08

Status: **Not started**

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

1. [ ] Implement persisted server/tool enable and disable operations in the chosen scope. Tool policy is default allow: retain disabled names across discovery changes/restarts and enable new tools automatically. Server disable takes precedence. Define errors for unknown server/tool targets.
2. [ ] Enforce effective policy before discovery/dispatch and again at the execution boundary where needed. Disabled direct calls fail without implicit enabling; define policy-change races with already-dispatched calls. Disable does not revoke or cancel a tool call already executed.
3. [ ] Implement `tools`, `tools <server>`, `tools <server> --all`, and `tool <server> <tool>`. Keep full schemas in detail output only. Include disabled state where requested. Expose per-server discovery failures and incomplete results with a nonzero status rather than hiding outages.
4. [ ] Implement complete discovery freshness rules for shorthand resolution. Search only enabled servers/tools; exactly one match permits invocation only when uniqueness is established across all eligible servers. On collision list qualified candidates, on no match fail, and on any incomplete discovery refuse to infer uniqueness. Qualified calls never require unrelated server discovery.
5. [ ] Implement all inline/file/stdin call forms with exactly one JSON argument source, approved root/schema validation behavior and useful read/parse errors. Never echo secret input in diagnostics. Ensure invalid input fails before a tool invocation and local qualified calls auto-start only when enabled/trusted.
6. [ ] Write the full MCP result envelope once to stdout. Preserve text, structured and non-text content; preserve tool-error payloads while returning the selected nonzero code. Handle broken pipes and serialization failures without retrying the call. Do not introduce automatic result saving or a preview invocation flag.
7. [ ] Implement built-in `shape <file>` and `shape -` over saved JSON, with field/type/array-length output and no scalar values. Define heterogeneous-array representation and explicit depth/width/size limits; report omitted structure. Shape must not load configuration, start the supervisor, access auth or invoke any server.
8. [ ] Add call-once/save/shape regression tests, persistence tests and exact machine-output/exit-code assertions for all public tool commands.

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

## Handoff (update whenever work stops)

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
