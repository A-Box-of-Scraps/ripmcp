# ripmcp

ripmcp is a CLI for managing and invoking MCP servers, minus the bloated ceremony.
It brings the power of MCP to AI agent harnesses built on the idea that Bash Is All You Need.

# Introduction

ripmcp brings the Model Context Protocol (MCP) to the command line. It manages local
and remote servers, discovers their tools, shows input schemas, and invokes tools
with JSON arguments. An agent harness with shell access can use MCP through
ordinary commands instead of needing its own MCP client integration (like Pi).

The idea is **CLIs over dedicated tool integrations**: keep Bash as the agent's
interface and discover capabilities when they are needed, rather than loading
every server's tool definitions into the agent's context up front. List tools with
`ripmcp tools`, inspect one with `ripmcp tool`, then invoke it with `ripmcp call`.

Shell workflows apply naturally: read arguments from a file or stdin, redirect
results to disk, and process saved JSON with tools such as `jq`. Use `ripmcp shape`
to inspect a result's structure without flooding the context with its contents or
calling the server again. The same commands work in an interactive terminal,
scripts, and agent harnesses.

ripmcp handles the MCP-specific work behind those commands: server installation,
authentication, project-scoped configuration and trust, and tool enable/disable
controls. Local servers can stay running and be reused across CLI invocations.
Bash is the interface; ripmcp manages the connections and server lifecycle.

<details>
<summary>Explore all commands and options</summary>

`<...>` marks a value to replace; `[...]` marks optional syntax. These are command
templates, not commands to paste unchanged. See the [command reference](docs/reference/cli.md)
for constraints and detailed behavior.

```sh
# Install a local server or import a local/remote server configuration
ripmcp install <server> --npx <package> [-- <args>...]
ripmcp install <server> --uvx <package> [-- <args>...]
ripmcp install <server> --docker <image> [-- <args>...]
ripmcp install <server> --config <file>                # Import one native server object
ripmcp install <server> --npx <package> --skip-verify  # Skip connection/tool discovery; works with any source

ripmcp uninstall <server>             # Stop and unregister; preserve installed data
ripmcp uninstall <server> --clean     # Also remove exclusively owned resources, with confirmation
ripmcp uninstall <server> --clean -y  # Same cleanup without prompting

# Scope: install, uninstall, enable, disable, and auth configure accept
# --user (the default) or --project (requires a discovered, trusted project)
ripmcp trust  # Review and trust the current project configuration
ripmcp install <server> --config <file> --project

ripmcp servers         # List configured servers and status without starting them
ripmcp start <server>  # Start and retain a local server for reuse
ripmcp stop <server>   # Stop a local server

ripmcp enable <server>          # Enable a server
ripmcp disable <server>         # Block future use; does not stop a running server
ripmcp enable <server> <tool>   # Enable one tool
ripmcp disable <server> <tool>  # Disable one tool

ripmcp tools                 # Discover enabled tools across enabled servers
ripmcp tools <server>        # Discover one server's enabled tools
ripmcp tools <server> --all  # Include disabled tools
ripmcp tool <server> <tool>  # Show the full tool definition, including its input schema

ripmcp call <server> <tool> '<json>'                # Pass a JSON argument object; use '{}' for no arguments
ripmcp call <server> <tool> --input <file>          # Read arguments from a JSON file
ripmcp call <server> <tool> --input -               # Read arguments from stdin
ripmcp call <tool> '<json>'                         # Omit the server only when discovery finds one unique match
ripmcp call <tool> --input <file>
ripmcp call <tool> --input -
ripmcp call <server> <tool> '<json>' --interactive  # Handle structured URL interaction; works with any call form

ripmcp shape <file> [--depth <depth>] [--width <width>]  # Inspect saved JSON offline; defaults: depth 8, width 100
ripmcp shape - [--depth <depth>] [--width <width>]       # Inspect JSON from stdin

# Configure credentials offline for remote HTTPS servers
ripmcp auth configure <server> --bearer                 # Prompt for a token and store it securely
ripmcp auth configure <server> --bearer-env <variable>  # Read a token from the environment when needed
ripmcp auth configure <server> --header <header>        # Prompt for a custom credential header value
ripmcp auth configure <server> --header <header> --header-env <variable>
ripmcp auth configure <server> --oauth-client-id <id> --issuer <url>
# OAuth configuration also accepts:
#   --client-secret                         Prompt for a client secret
#   --client-secret-env <variable>          Use an environment reference instead of prompting
#   --token-endpoint-auth-method <method>   none, client_secret_post, or client_secret_basic
#   --scope <scope>                         Repeat for multiple OAuth scopes

ripmcp auth login <server>   # Run remote OAuth login
ripmcp auth status <server>  # Inspect authentication status
ripmcp auth logout <server>  # Remove locally stored authentication

ripmcp --timeout <seconds> <command>  # Override the operation deadline; also accepted after subcommands
ripmcp --help                         # Show top-level help
ripmcp <command> --help               # Show command-specific arguments and options
ripmcp --version                      # Show the installed version
# Help and version also accept -h and -V, respectively

ripmcp --uninstall-everything     # Remove ripmcp and its owned resources, with confirmation
ripmcp --uninstall-everything -y  # Same self-removal without prompting
```

</details>

# Documentation

Start [HERE](docs/README.md)!

# License

Licensed under the [MIT License](LICENSE) by Titouan Réthoré.
