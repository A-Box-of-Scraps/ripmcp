# Tool discovery, invocation, and result inspection

Status: proposed UX, not committed command syntax.

## Discovery and invocation

Illustrative commands:

```sh
ripmcp servers
ripmcp tools
ripmcp tools github
ripmcp tool github search_issues
ripmcp call github search_issues '{"query":"repo:owner/project is:open"}'
ripmcp call github search_issues --input arguments.json
printf '%s\n' '{"query":"repo:owner/project is:open"}' | ripmcp call github search_issues --input -
```

Proposal: tool lists show concise descriptions; tool inspection shows the input
schema. Qualify tool names with the server to avoid collisions. Keep full schemas
out of the default all-server listing.

## Unqualified invocation

User-approved command surface gains a requested shorthand:

```sh
ripmcp call <tool> '<json>'
ripmcp call <tool> --input arguments.json
ripmcp call <tool> --input -
```

Resolve an unqualified tool name to a server when unique. If multiple servers
provide it, fail without invoking any tool, list the matching server names, and
instruct the caller to retry with ripmcp call <server> <tool>. Never select the
first match arbitrarily. No match also produces an explicit error.

Agreed resolution boundaries: consider only enabled servers and enabled tools.
If discovery is incomplete, report that uniqueness cannot be established rather
than assuming unavailable servers have no matching tools. Explicitly qualified
calls do not require cross-server resolution.

## Large results

The important workflow is call once, inspect structure, then extract selected
data from the saved result. Repeating a tool call just to change its output view
can repeat side effects or return different data.

Candidate shell-native workflow:

```sh
ripmcp call github search_issues --input arguments.json > result.json
jsonshape < result.json
# Select fields from result.json after inspecting its shape.
```

The local jsonshape skill documents a shape-only view of field names, types, and
array lengths, without scalar values. This fits the requested inspection step.

Agreed direction: ordinary invocation returns the result, not a shape preview.
User is considering built-in shape inspection versus a repository skill teaching
agents to save results and use external jsonshape. jq need not be reimplemented.

Agreed: a standalone ripmcp shape command
inspects a saved file or stdin without invoking a server. Pair it with a skill
teaching call-once, save, inspect, and jq extraction. This avoids a mandatory
external jsonshape dependency without adding result-cache lifecycle machinery.

The shape command is agreed; shipping an accompanying agent skill remains a
proposal. Ordinary invocation continues to return the full result.

An invocation --preview flag is not agreed. If added, it must save the complete
result and report its location rather than discard it and require another call.
Do not call a tool twice just to obtain different views of the same result.

## Output contract to decide

- Preserve the MCP result envelope or expose a documented normalized format?
- How are text, structured data, and non-text content represented?
- Explicit file output only, or automatic saved-result handles for large results?
- How are preview size and depth bounded for very wide or deep structures?
- How are truncation and the full-result location reported without corrupting JSON?
- If ripmcp stores results, what are the permissions, retention, and cleanup rules?

Proposal: never silently truncate the saved result. Bound previews independently.
Separate protocol errors from tool-reported failures while preserving useful
failure content for inspection.
