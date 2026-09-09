# Authoritative v1 command surface

User-supplied command list, confirmed September 8, 2026. This is an implementation
contract for subsequent phases, not product documentation. It supersedes tentative
spellings in `../ideas/`. Parsing support does not mean a handler is implemented.

```sh
ripmcp install ...                         # Install a local server or register a remote configuration
ripmcp install ... --skip-verify           # Install without connection/tool-discovery verification

ripmcp uninstall <server>                  # Stop and unregister; preserve installed data
ripmcp uninstall <server> --clean          # Also remove exclusively owned resources, with confirmation
ripmcp uninstall <server> --clean -y       # Same cleanup without prompting

ripmcp enable <server>                     # Enable a server
ripmcp disable <server>                    # Disable a server
ripmcp enable <server> <tool>              # Enable a tool
ripmcp disable <server> <tool>             # Hide a tool and block its invocation

ripmcp start <server>                      # Start a local server explicitly
ripmcp stop <server>                       # Stop a local server
ripmcp servers                             # List configured servers and their status

ripmcp tools                              # List enabled tools across enabled servers
ripmcp tools <server>                      # List a server's enabled tools
ripmcp tools <server> --all                # Include disabled tools
ripmcp tool <server> <tool>                # Show tool details and input schema

ripmcp call <server> <tool> '<json>'        # Invoke with inline JSON; auto-start if needed
ripmcp call <server> <tool> --input <file>  # Read arguments from a JSON file
ripmcp call <server> <tool> --input -       # Read arguments from stdin
ripmcp call <tool> '<json>'
ripmcp call <tool> --input args.json
ripmcp call <tool> --input -
ripmcp call <server> <tool> '<json>' --interactive --timeout 180

ripmcp shape <file>                        # Inspect saved JSON structure without invoking anything

ripmcp auth login <server>                 # Wait for browser login to complete
ripmcp auth configure <server> --bearer    # Prompt for and securely store a raw bearer token
ripmcp auth configure <server> --bearer-env <name>
ripmcp auth configure <server> --header <name> [--header-env <variable>]
ripmcp auth configure <server> --oauth-client-id <id> --issuer <url>
ripmcp auth status <server>                # Show authentication status
ripmcp auth logout <server>                # Remove locally saved credentials

ripmcp trust                              # Trust the current project's .ripmcp configuration

ripmcp --uninstall-everything              # Remove ripmcp and owned resources, with confirmation
ripmcp --uninstall-everything -y           # Same removal without prompting; preserve project configs
```

## Approved additions and precise grammar

D03-D04 supply the installation source and mutation scope forms:

```sh
ripmcp install <server> --npx <package> [-- <args>...]
ripmcp install <server> --uvx <package> [-- <args>...]
ripmcp install <server> --docker <image> [-- <args>...]
ripmcp install <server> --config <file>
```

- Exactly one install source is required. `--config` reads one native server
  definition, not an entire config or a foreign-format import. Runtime arguments
  require `--` and cannot accompany `--config`. `--skip-verify` applies to all sources.
- `install`, `uninstall`, `enable`, and `disable` accept mutually exclusive
  `--user` / `--project`; omission means user scope. Neither flag applies to
  `trust`, lifecycle, discovery, invocation, shape, OAuth login/status/logout or
  self-removal. `auth configure` also accepts these scope flags.
- `trust` always targets the discovered current project. No path argument, `-y`,
  or implicit project creation is provided. Confirmation behavior is in contracts.md.
- Without `--input`, `call` takes exactly `tool JSON` or `server tool JSON`.
  With `--input`, it takes exactly `tool` or `server tool`. Arity alone determines
  qualification; the parser never queries discovery. Inline JSON must be an object.
  File/stdin input is represented as a typed source without being read at parse time.
  Tool names in the `--input` form are not reinterpreted as inline JSON.
- `shape -` is also supported under D06. `--depth` defaults to 8, range 1-64;
  `--width` defaults to 100, range 1-10000. Inspection never invokes tools.
- Global `--timeout <seconds>` accepts a positive integer before or after a
  subcommand. See contracts.md for configuration and deadline semantics.
- `auth configure` supports user/project scope and offline credential setup.
  OAuth options include `--client-secret`, `--client-secret-env`, repeated
  `--scope`, and `--token-endpoint-auth-method`. See
  [generic authentication](10-generic-authentication.md) for exact semantics,
  storage ownership, examples, and compatibility limits.
- `call --interactive` opts into URL elicitation for server-managed login or
  other external user interaction. Validated URLs and instructions are printed
  immediately to stderr; the user opens the URL manually. The command continues
  the structured MCP request and waits for the final result under the original
  deadline. Ctrl-C cancels. Without this flag, input-required results fail without
  continuation. Form input, sampling and plain-text login parsing are unsupported.
  This does not change remote OAuth `auth login`, which already waits, or add
  `auth login` support for local servers.
- `-y` is accepted only with clean server uninstall or self-uninstall. It skips
  confirmation, never expands ownership or deletion scope. Self-uninstall cannot
  accompany another command. `--clean` and `--include-projects` are not self-uninstall flags.
- `--help` and `--version` succeed without reading configuration or starting a server.
