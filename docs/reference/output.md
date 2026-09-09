# Output, exit codes, and limits

[Documentation](../README.md) / Reference

## Streams and tool results

Data responses are one complete JSON value followed by a newline on stdout.
Diagnostics, login instructions, previews, and prompts go to stderr. Help and
version output are text exceptions. Errors before a report is available can leave
stdout empty. A failed stdout write can leave partial output.

`call` returns the full MCP **tool-result** envelope, not the JSON-RPC transport
wrapper or a text-only projection. Content, structured content, extensions, and
exact JSON numbers are preserved as values; whitespace and key order are not
preservation guarantees. No result cache, automatic save, or silent truncation is
provided.

If `isError` is true, the envelope is written before exit 7. Output failure takes
precedence and exits 1. Early shorthand errors can instead write a candidate or
discovery report; not every nonempty `call` output is a tool result.

## Other reports

Non-call reports use `schema_version: 1`. Do not expect one universal report shape:

| Command                    | Main fields                                                                                  |
| -------------------------- | -------------------------------------------------------------------------------------------- |
| `servers`                  | `servers`: name, provenance, enabled, trust_required, process_state, health                  |
| `tools`                    | `tools`: server, name, enabled, optional description/title; `errors` array                   |
| `tool`                     | Full definition under `tool`                                                                 |
| Ambiguous shorthand        | `candidates`: server/tool pairs                                                              |
| `install`                  | `installation`: identity, scope, preparation, verification, retention and mutation reporting |
| `enable`, `disable`        | `actions`, `shadowed_by_trusted_project`, `project_reapproval_required`                      |
| `auth configure`           | `auth`, mutation flags; remote_validity_checked is false                                     |
| `auth status/login/logout` | `auth`: server, state, sharing and remote-validity indicators                                |
| `trust`                    | `trust`: approved project information                                                        |
| `uninstall`                | `completed`, `failures`, `plan`; plan includes preserved resources and retry vector/cwd      |
| Self-removal               | `completed`, `failures`, `plan`; retained recovery state on incomplete removal               |
| `shape`                    | `shape` and `limits`                                                                         |

Do not treat an empty/partial discovery report as proof that unavailable servers
have no tools. `tools` writes valid entries and safe failure identities before
exiting 8 on incomplete discovery.

### Server status

`enabled` is configuration, not a health check. `process_state` can be `running`,
`stopped`, `busy`, `failed`, `unknown`, `cleanup_required`, or `not_managed`.
`health` can be `healthy`, `failed`, `unknown`, `unverified`, or `stale`.

Remote entries are `not_managed`/`unknown`. Local observations become stale after
60 seconds or a configuration revision change; passive listing does not refresh
them. A running process can have stale, unknown, or failed health.

### Authentication status

OAuth states include `saved`, `signed_out`, `expired`, and `login_required`.
Saved means local credentials exist, not that a provider currently accepts them.
Automatic OAuth credentials can be shared by identical endpoint aliases; explicit
registration partitions credentials by endpoint and registration configuration.

Bearer status succeeds with `state: "available"` only after local resolution and
syntax checks; missing credentials are errors. It does not contact the provider.
Bearer status reports `method: "bearer"`. Custom header collections are not
managed by these status/logout operations.

## Shape reports

Shape reads any valid saved JSON value and reports structure without scalar
values:

- Every node has `type`: `object`, `array`, `string`, `number`, `boolean`, or `null`.
- Objects add `fields`, mapping names to node shapes.
- Arrays add exact `length` and ordered `items`, one shape per inspected element.
  Heterogeneous elements are not merged or inferred from unseen items.
- Containers have `truncated`; if children are omitted, `omitted` gives the exact
  number of omitted immediate children.

Root depth is zero. At the selected depth a container retains its type and array
length but omits children. Width applies per container. The global node budget can
also stop inspection. A bounded report exits 0; invalid/oversized JSON exits 2
without a shape report. The saved file is never modified.

Shape is not a general redaction tool: field names and array lengths remain
visible, and field names themselves might contain sensitive information.

## Exit codes

| Code | Meaning                                                                   |
| ---- | ------------------------------------------------------------------------- |
| 0    | Success, including help/version                                           |
| 1    | Internal or I/O failure, including stdout write failure                   |
| 2    | Invalid CLI or tool input; missing/ambiguous tool name                    |
| 3    | Configuration, policy, project trust, or confirmation requirement failure |
| 4    | Connection or transport failure                                           |
| 5    | Protocol error, violation, or version mismatch                            |
| 6    | Authentication required or failed                                         |
| 7    | Tool-reported failure; inspect the emitted envelope                       |
| 8    | Partial discovery or incomplete requested cleanup                         |
| 9    | Deadline exceeded                                                         |
| 10   | Unsupported platform or capability                                        |
| 130  | Cancelled, including declined cleanup confirmation                        |

## Resource limits

These are implementation limits, not an assurance of maximum concurrent memory
usage. Only shape depth/width and the operation timeout have public CLI options.

| Boundary                              | Limit                                                  |
| ------------------------------------- | ------------------------------------------------------ |
| Config storage / native install input | 4 MiB                                                  |
| Call file/stdin input                 | 16 MiB                                                 |
| Shared strict JSON parser             | Nesting bound 128; duplicate object keys rejected      |
| Shape input                           | 64 MiB                                                 |
| Shape depth                           | Default 8; CLI range 1-64                              |
| Shape width                           | Default 100; CLI range 1-10000                         |
| Shape inspection nodes                | 100000                                                 |
| MCP frame or SSE event                | 16 MiB by default                                      |
| Cumulative discovery                  | 32 MiB, 10000 tools, 1000 pages by default             |
| Supervisor IPC operation frame        | 64 MiB                                                 |
| Interactive continuation              | At most 16 request rounds and 16 URL prompts per round |

Inline arguments also face shell/OS argument limits. Transport limit failures
return errors instead of shortened results. Shape output bounds do not bypass
input or protocol limits.

## Deadlines and cancellation

One monotonic operation budget includes waits, preparation, connection, discovery,
invocation, and any interactive continuation. Progress does not reset it. Explicit
OAuth login and credential setup include time spent waiting for the user and
secure storage. Destructive confirmation happens before the mutation deadline.

Ctrl-C requests cancellation and exits 130; deadline expiry exits 9. Neither
proves server-side effects were rolled back. ripmcp never automatically replays a
tool invocation after uncertain delivery. Cancelling one reused local call does
not kill the shared server. Process cleanup may need an additional bounded grace
period. Synchronous filesystem/JSON work cannot always be preempted immediately.

## Implementation source

[Output writer](../../src/output.rs), [errors](../../src/error.rs),
[shape](../../src/shape.rs), [tool reports](../../src/tools/mod.rs),
[server status](../../src/supervisor/manager/status.rs),
[cleanup reports](../../src/cleanup/mod.rs), and [shape tests](../../tests/shape.rs).
