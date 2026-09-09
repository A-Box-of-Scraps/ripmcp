# Discover, invoke, and control tools

[Documentation](../README.md) / How-to guides

## Discover before calling

```sh
ripmcp servers
ripmcp tools
ripmcp tools remote
ripmcp tool remote TOOL_NAME
```

`servers` is passive. Tool discovery connects to enabled servers and can start
installed local servers. Tool lists contain concise metadata; `tool` returns the
full definition and input schema. Check nonzero exits and the `errors` array:
cross-server discovery can return valid entries alongside failures.

## Supply arguments

Use exactly one JSON object, inline, from a file, or from stdin:

```sh
ripmcp call remote TOOL_NAME '{}'
ripmcp call remote TOOL_NAME --input arguments.json
ripmcp call remote TOOL_NAME --input - < arguments.json
```

These are alternative invocations, not a sequence to run on the same task. Replace
`{}` with the tool's required arguments. Prefer files or stdin for sensitive input;
inline JSON is visible to shell history and process-argument inspection.

For an unqualified name, omit the server:

```sh
ripmcp call TOOL_NAME --input arguments.json
```

Shorthand discovers all enabled servers and tools. It invokes only when discovery
is complete and exactly one match exists. Collisions return candidates without a
call. Unavailable or untrusted eligible servers make discovery incomplete, not
empty. Use a qualified call to avoid unrelated discovery failures.

## Save once, inspect repeatedly

```sh
umask 077
ripmcp call remote TOOL_NAME --input arguments.json > result.json
```

Check the exit status immediately. Exit 7 retains a tool-error envelope; transport
or protocol failure can mean completion is unknown. Do not blindly retry a call
that might have side effects.

For a saved result:

```sh
ripmcp shape result.json --depth 4 --width 20
```

Then select fields from the same file with your JSON tool. Do not repeat the call
to change its view. See the [guided extraction example](../tutorials/inspect-json.md)
and [output contract](../reference/output.md).

## Hide and block tools

```sh
ripmcp disable remote TOOL_NAME
ripmcp tools remote --all
ripmcp enable remote TOOL_NAME
```

Disabled tools are hidden from normal discovery and blocked on direct calls.
`--all` includes disabled tools, not disabled servers. New tools are enabled by
default. Disabled names remain in policy if the server stops advertising them.

These commands default to user scope. Use `--project` for a project definition and
renew trust afterward. Except for already retained disabled names, policy changes
need live discovery to validate the tool.

## Control a local server

```sh
ripmcp start local
ripmcp stop local
```

Enabled local servers also start automatically when needed for discovery or calls.
Separate CLI processes reuse the owned server. There is no idle shutdown. Remote
servers do not support start/stop.

To block future use and stop an existing local process:

```sh
ripmcp disable local
ripmcp stop local
```

Disabling alone does not stop a process or cancel an already transmitted request.
Enabling alone does not start it. Explicit start of a disabled server fails.

For URL interaction and longer bounded calls, see
[authentication and interaction](authenticate.md#supply-local-credentials-or-complete-tool-interaction).
