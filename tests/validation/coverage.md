# Phase 09 command and fault coverage

This is test evidence, not product documentation. Paths are relative to the
repository root. `tests/validation/coverage.rs` checks all confirmed forms, scope
acceptance/rejection and timeout grammar, and fails on an unlisted public
subcommand. Parsing is not evidence that a handler succeeds.

| Command/form | Success evidence | Validation/failure evidence |
| --- | --- | --- |
| install npx, uvx, Docker; literal args; skip-verify | `tests/install/local.rs`, `tests/validation/local.rs` | missing runtimes, bad pins, failed preparation, strict import in `tests/install/local.rs`; cancellation and interrupted publication in `tests/install/transactions.rs` |
| install native local/remote config | `tests/install/local.rs`, `tests/install/remote.rs` | duplicate keys, special files, credential headers, unreachable/auth-required endpoints in the same suites |
| install/uninstall/enable/disable default, user, project | grammar matrix; `tests/install/transactions.rs`, `tests/tools.rs`, `tests/uninstall/recovery.rs` | conflicting flags; missing project; changed trust; shadowing; concurrent writes in those suites and `tests/config.rs` |
| ordinary uninstall | `tests/uninstall.rs`, combined reinstall in `tests/validation/local.rs` | missing/ambiguous registration, failed owned stop in `tests/uninstall/recovery.rs` |
| clean uninstall, with/without -y | `tests/uninstall.rs`, `tests/uninstall/terminal.rs` | no terminal, decline, changed preview, permissions, substituted paths, incomplete cleanup and retry in these suites and `tests/uninstall/recovery.rs` |
| server enable/disable | `tests/tools.rs`, `tests/validation/local.rs` | missing server; disabled start/call; retained process and tool policy in the same suites |
| tool enable/disable | `tests/tools.rs`, `tests/validation/local.rs` | missing/disappeared tool, scope shadowing, policy changes during discovery/after dispatch in `tests/tools.rs` |
| start/stop local | `tests/lifecycle/local.rs`, `tests/validation/local.rs` | missing installation, disabled/untrusted, stopped/failed owner, cancellation in `tests/lifecycle/recovery.rs` |
| start/stop remote | intentionally unsupported | exit 10 without network requests in `tests/validation/remote.rs` |
| servers | `tests/config.rs`, `tests/lifecycle/local.rs`, both validation lifecycles | malformed config; passive redaction, trust-required and stopped/not-managed status in the same suites |
| tools, tools server, tools server --all | `tests/tools.rs`, both validation lifecycles | incomplete discovery, disabled server/tool, unavailable peer in `tests/tools.rs`, `tests/auth.rs` |
| tool server tool | `tests/tools.rs`, `tests/validation/local.rs` | missing names, policy and protocol errors in `tests/cli.rs`, `tests/lifecycle/input.rs`, `tests/mcp_stdio.rs` |
| qualified/shorthand call, inline/file/stdin | all six forms in `tests/tools.rs::call_once_save_shape_and_reuse_with_all_input_forms`; installed adapters in `tests/validation/local.rs`; remote in `tests/auth.rs` | malformed/missing/oversized input; ambiguous or incomplete discovery; policy/trust races; tool errors in `tests/tools.rs`, `tests/lifecycle/input.rs`, `tests/mcp_http.rs` |
| shape file/stdin and bounds | `tests/shape.rs`, saved tool result in `tests/tools.rs` | invalid/oversized/deep input, depth/width/node omissions in `tests/shape.rs`; offline despite malformed config |
| auth login/status/logout | provider/browser/callback/fake secure-store lifecycle in `src/auth/tests.rs` and `src/auth/tests/lifecycle.rs`; status serialization in `src/auth/command.rs` | local/non-OAuth/untrusted targets and unavailable Secret Service in `tests/auth.rs`; timeout, cancellation, refresh, logout races and endpoint binding in provider tests |
| trust | terminal approval/default-no in `tests/config/terminal.rs` | nonterminal, changed bytes/root/endpoint, symlink and forged trust in `tests/config/trust.rs`, `tests/cli.rs` |
| --uninstall-everything, with/without -y | copied owned executable, terminal approval in `tests/self_uninstall.rs` | decline, unknown/package-managed/substituted binary, unavailable secure store, owned-resource failure, concurrent auto-start in the same suite |
| help/version | `tests/cli.rs` and release smoke | no configuration or execution required |

## Cross-cutting evidence

| Risk | Evidence |
| --- | --- |
| No replay, no implicit start | request/launch counts in both validation lifecycles; disconnect, timeout, SIGINT and broken stdout tests in `tests/tools.rs`, `tests/mcp_http.rs`, `tests/mcp_stdio.rs`, `tests/lifecycle/recovery.rs` |
| Running supervisor with revoked trust | `tests/lifecycle/recovery.rs::changed_project_trust_is_enforced_even_by_an_existing_supervisor`; IPC policy rechecks in `tests/lifecycle/ipc.rs` |
| Concurrent config publication | `tests/install/transactions.rs`, `tests/config/storage.rs` |
| Interrupted install/cleanup | pending publication/config substitution in `tests/install/transactions.rs`; quarantine recovery and maintenance exclusion in `tests/uninstall/recovery.rs` |
| PID/socket reuse and foreign process preservation | `tests/supervisor/ipc.rs`, `tests/supervisor.rs`, `tests/lifecycle/recovery.rs` |
| Disk full and permission failure | `/dev/full` no-replay test in `tests/validation/failures.rs`; opt-in private 64 KiB tmpfs test covers actual ENOSPC, old committed bytes, failed CLI mutation, successful retry; `tests/uninstall/recovery.rs` covers permission failures |
| Secret and deletion boundaries | `tests/auth.rs`, provider tests, `tests/config/trust.rs`, `tests/install/local.rs`, `tests/uninstall.rs`, `tests/self_uninstall.rs` |
| Large results and shape bounds | 2 MiB full envelope in `tests/tools.rs`; explicit transport limits in `tests/mcp_stdio.rs` and `tests/mcp_http.rs`; node/input bounds in `tests/shape.rs` |

## Remaining integration gaps

- Authenticated login-to-call-to-refresh-to-logout is covered at separate provider
  and HTTP/CLI seams, not one end-to-end CLI test with TLS and an isolated real
  Secret Service adapter. Endpoint binding and project reapproval are likewise
  separate tests. Do not describe this as a completed full remote OAuth lifecycle.
- Real-runtime smoke is opt-in in `tests/install/local.rs`. Default tests use fake
  npx/uvx/Docker executables. An ignored test is not a successful runtime check.
- The bounded-result tests establish explicit size rejection and no truncation;
  they are not a measured peak-memory stress test of concurrent maximum-size
  supervisor replies.
