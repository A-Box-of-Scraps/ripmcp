# Command reference

[Documentation](../README.md) / Reference

Notation: uppercase names are values to replace; `[ ... ]` is optional syntax.
Use `ripmcp --help` or `ripmcp COMMAND --help` for parser-generated options.

## Global options

| Option                        | Meaning                                                                    |
| ----------------------------- | -------------------------------------------------------------------------- |
| `--timeout SECONDS`           | Positive integer operation budget; accepted before or after the subcommand |
| `--help`, `-h`                | Print help without configuration or server startup                         |
| `--version`, `-V`             | Print the binary version without configuration or server startup           |
| `--uninstall-everything [-y]` | Self-removal; cannot accompany a subcommand                                |

Operation timeout defaults to 60 seconds. OAuth login and credential configuration
default to 300 seconds. Trusted configuration can replace these defaults; CLI
`--timeout` wins. `shape` instead uses its own 60-second default and ignores config.
See [deadline behavior](output.md#deadlines-and-cancellation).

## Installation and policy

```text
ripmcp install SERVER --npx PACKAGE [--user | --project] [--skip-verify] [-- ARGS...]
ripmcp install SERVER --uvx PACKAGE [--user | --project] [--skip-verify] [-- ARGS...]
ripmcp install SERVER --docker IMAGE [--user | --project] [--skip-verify] [-- ARGS...]
ripmcp install SERVER --config FILE [--user | --project] [--skip-verify]
ripmcp uninstall SERVER [--user | --project] [--clean [-y]]
ripmcp enable SERVER [TOOL] [--user | --project]
ripmcp disable SERVER [TOOL] [--user | --project]
```

- Exactly one install source is required. Runtime arguments require `--` and
  cannot accompany `--config`.
- `--config` imports one [native server object](configuration.md#server-object).
- `--skip-verify` skips connection/discovery, not preparation or input validation.
- Scope flags are mutually exclusive. Omission means user scope, not current
  project scope. Project writes require a discovered, trusted project.
- Duplicate server names in the target scope fail. There is no update flag.
- `-y` on server uninstall requires `--clean`. It only skips confirmation.
- Disable blocks future use but does not stop a process or cancel sent calls.

Procedures: [install](../how-to/install.md), [policy](../how-to/tools.md),
[remove](../how-to/remove.md).

## Lifecycle and discovery

```text
ripmcp servers
ripmcp start SERVER
ripmcp stop SERVER
ripmcp tools [SERVER]
ripmcp tools SERVER --all
ripmcp tool SERVER TOOL
ripmcp trust
```

`servers` is passive. `tools` and `tool` perform fresh discovery and can start
enabled local servers. `--all` requires a server and includes disabled tools only.
`tool` returns the full tool definition but does not bypass disabled policy.
Remote start/stop are unsupported; local stop can stop an already-owned disabled
or now-untrusted instance without executing the edited definition.

These commands select effective configuration; they have no scope flags. `trust`
has no path argument, `-y`, or project-creation behavior.

## Invocation

```text
ripmcp call SERVER TOOL JSON
ripmcp call SERVER TOOL --input FILE
ripmcp call SERVER TOOL --input -
ripmcp call TOOL JSON
ripmcp call TOOL --input FILE
ripmcp call TOOL --input -
```

All forms accept `--interactive` for structured URL interaction. Without `--input`,
the final positional is a JSON object. With `--input`, positionals are only the
optional server and required tool. The number of positionals determines whether
the call is qualified; parsing never guesses names through discovery.

Exactly one JSON argument object is required, including `{}` for a tool with no
arguments. Input is parsed before shorthand discovery. General JSON Schema
validation remains server-side.

Shorthand considers enabled servers and enabled tools. Exactly one match and
complete discovery are required. Ambiguity returns candidates and exit 2;
incomplete discovery returns a discovery report and exit 8. Neither invokes a tool.

`--interactive` continues structured URL input requests under the original deadline.
It does not parse ordinary login text or retry uncertain delivery. URLs and
instructions appear on stderr; the final result appears on stdout.

## Offline shape

```text
ripmcp shape FILE [--depth DEPTH] [--width WIDTH]
ripmcp shape - [--depth DEPTH] [--width WIDTH]
```

| Option    | Default | Accepted range |
| --------- | ------- | -------------- |
| `--depth` | 8       | 1-64           |
| `--width` | 100     | 1-10000        |

Any JSON root is accepted. Shape reads no configuration or credentials and invokes
no server. Bounds affect the report only; see [shape output](output.md#shape-reports).

## Authentication

```text
ripmcp auth configure SERVER --bearer [--user | --project]
ripmcp auth configure SERVER --bearer-env VARIABLE [--user | --project]
ripmcp auth configure SERVER --header HEADER [--header-env VARIABLE] [--user | --project]
ripmcp auth configure SERVER --oauth-client-id ID --issuer URL [OPTIONS] [--user | --project]
ripmcp auth login SERVER
ripmcp auth status SERVER
ripmcp auth logout SERVER
```

Exactly one configure source is required: bearer prompt, bearer environment,
header, or OAuth client ID. Configure is offline and targets remote HTTPS servers.
Only configure accepts scope flags. Login/status/logout use the effective server.

OAuth configure options:

| Option                                | Constraint                                              |
| ------------------------------------- | ------------------------------------------------------- |
| `--issuer URL`                        | Required with client ID; exact, query-free HTTPS issuer |
| `--client-secret`                     | Hidden prompt; conflicts with `--client-secret-env`     |
| `--client-secret-env VARIABLE`        | Secret reference, resolved when needed                  |
| `--token-endpoint-auth-method METHOD` | `none`, `client_secret_post`, or `client_secret_basic`  |
| `--scope SCOPE`                       | Repeatable; one scope token per occurrence              |

Secret, method, and scope options require a client ID. Method `none` requires no
secret; Post/Basic require a secret. The helper defaults to Post with a secret,
otherwise none. Login is for remote OAuth, not bearer or local server credentials.

Procedures and prerequisites: [authentication](../how-to/authenticate.md).

## Implementation source

[CLI parser](../../src/cli.rs), [dispatch](../../src/lib.rs),
[call grammar](../../src/call.rs), and [CLI tests](../../tests/cli.rs).
Internal supervisor/guard commands are not a public administration interface.
