# Troubleshoot a command

[Documentation](../README.md) / How-to guides

Start with the exit code and stderr diagnostic. `ripmcp servers` reads saved state
without starting servers or testing endpoint health. Use `ripmcp tools SERVER`
when you explicitly want a connection/discovery check; this can start a local
server but does not invoke a tool.

| Symptom                                                    | Check or action                                                                                                                                                 |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Invalid command arguments (2)                              | Run the command's `--help`; check the [grammar](../reference/cli.md). Call arguments must be exactly one JSON object.                                           |
| Project trust required (3)                                 | Inspect the nearest selected `.ripmcp/config.json`, then run `ripmcp trust` in a terminal. There is no unattended trust flag.                                   |
| Wrong server or missing scoped target (3)                  | Inspect `servers` provenance. Writes default to user scope; project definitions replace same-named user definitions.                                            |
| Local server requires a matching prepared installation (3) | Use `install`, not only a hand-written local configuration. Check whether runtime/package edits or a moved runtime invalidate the recorded installation.        |
| Server/tool disabled (3)                                   | Enable the correct target scope. Renew project trust if its bytes changed. `--all` is not a bypass.                                                             |
| Transport failure (4)                                      | Check the endpoint, runtime prerequisites, and server compatibility. Completion of a sent call may be unknown; do not automatically replay it.                  |
| Protocol error (5)                                         | The server must support the exact implemented revision and transport profile. Older initialization/session behavior does not silently fall back.                |
| Authentication required or failed (6)                      | Use the appropriate [authentication guide](authenticate.md). Check the invoking environment, Secret Service, issuer, scopes, and endpoint binding.              |
| Tool-reported failure (7)                                  | Inspect the saved full result. The tool's useful error content is on stdout, not replaced by the diagnostic.                                                    |
| Incomplete discovery (8)                                   | Inspect the report's `errors`. Use a qualified call when unrelated servers prevent shorthand resolution.                                                        |
| Incomplete cleanup (8)                                     | Retain the report and journal, correct the cause, and use the [cleanup retry procedure](remove.md#recover-from-partial-cleanup).                                |
| Deadline exceeded (9)                                      | Increase global `--timeout` if appropriate. The budget includes preparation, queueing, discovery, and interaction; a timed-out call may have had effects.       |
| Unsupported capability (10)                                | Check the [compatibility table](../reference/compatibility.md). Remote start/stop, local OAuth login, and unsupported input-required flows are not implemented. |

## OAuth login will not complete

- Check that the default Secret Service collection exists and is unlocked on the
  session bus. ripmcp does not unlock it or fall back to a file.
- Check that local port 42813 is free and the provider registered the exact
  callback. There is no alternate-port or remote-headless redirect fallback.
- If no browser opens, use the printed URL manually on the same machine while
  the command runs. The printed URL is the authorization URL, not a token.
- Private-network OAuth providers, missing S256 PKCE support, and incompatible
  client registration are explicit limitations, not reasons to disable TLS checks.
- `auth status` reports local state, not provider acceptance. After changing
  credentials or registration, use explicit login or discovery as appropriate.

## Bearer logout does not clear credentials

Environment and external-keyring references are externally managed. Logout exits
10; rotate or remove the value at its source. For a managed `stored` reference,
bearer logout requires the registration to be enabled and trusted first. Enabling
a server does not start it or make a network request. See
[credential rotation](authenticate.md#sign-out-and-rotate-credentials).

## Status says running but health is stale

Enablement, process ownership, and health are different fields. A passive list
does not probe the server. Health observations age or become stale when configuration
changes. Request discovery to make an active check. `cleanup_required` indicates
retained lifecycle cleanup evidence, not an invitation to delete a socket or kill
a PID from a file.

## A pipe or stdout write failed

A call can finish at the server before output fails locally. Exit 1 and an empty
or partial destination file do not prove the call never happened. Check the
server's application state before deciding whether a repeat is safe.

## Storage is rejected

Keep XDG locations absolute and sensitive state private. State directories and
files must satisfy ownership, type, no-symlink, and permission checks. Inspect the
specific path and its provenance rather than deleting state or blindly relaxing
permissions. Unknown/incompatible state is deliberately preserved.

For a bug report, include the ripmcp version, Linux/runtime versions, command shape,
exit code, and a redacted diagnostic. Do not include raw tokens, client secrets,
callback URLs with codes, or sensitive argument/result files.
