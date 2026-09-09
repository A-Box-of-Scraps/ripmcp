# V1 scope

## User direction

- Support MCP specification 2026-07-28.
- Keep the CLI small and avoid ceremony.
- Install and uninstall servers.
- Enable and disable servers and individual tools; both are required in v1.
- Start and stop local servers.
- List installed servers and their status.
- List tools across servers or for one server.
- Invoke tools with JSON arguments.
- Preview result structure before selecting data, potentially using jsonshape.

## Protocol target

Checked on September 8, 2026: the official specification landing page resolves
to revision 2026-07-28, matching the requested target.

Source supplied by the user:
`https://modelcontextprotocol.io/specification/2026-07-28`

Proposal: define a tools-focused client profile rather than promise every MCP
feature. Document required protocol behavior, supported transports, optional
capabilities, and unsupported features. Advertise only capabilities implemented.
Decide older-version compatibility explicitly, not through silent fallback.

## Proposed essentials

- Tool input-schema inspection before invocation.
- JSON input from an argument, stdin, or a file.
- Stable machine-readable output, with diagnostics on stderr.
- Defined exit codes for configuration, connection, protocol, and tool failures.
- Timeouts and cancellation, without automatic retries of tool calls that might
  have side effects.
- Secret references rather than credentials embedded in displayed commands.
- Clear errors for missing authentication and unsupported capabilities.
- Partial discovery results when one server is unavailable, with failures exposed.

These are proposals, not additional agreed requirements.

## Proposed deferrals

- GUI, agent loop, workflow engine, or plugin framework.
- General-purpose package management across every server runtime.
- Registry browsing and automatic server updates.
- Resources, prompts, and optional extensions unless a concrete workflow needs them.
- A custom JSON query language; prefer existing shell tools.

## Assessment

The requested workflow is a good v1 direction. The main scope risks are package
installation, persistent process management, and remote authentication. Agree on
their boundaries before selecting an implementation architecture.
