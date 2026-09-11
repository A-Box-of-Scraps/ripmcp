# Make a first call to your remote server

[Documentation](../README.md) / How-to guides

Register your remote server, inspect a tool, and save one invocation result.
This guide assumes you can choose a compatible endpoint and safe tool. Discovery
does not invoke tools.

## Before you start

You need:

- A Linux shell with [ripmcp installed](install.md).
- An HTTPS MCP server that supports ripmcp's
  [protocol profile](../reference/compatibility.md).
- A tool on that server that you can safely run, and any required credentials.

The endpoint and tool below are placeholders, not a public demo service. Use your
server's endpoint and actual tool name. If you do not have a compatible server,
start with the [offline tutorial](../tutorials/inspect-json.md) instead.

## 1. Register the endpoint

Work outside any project with a `.ripmcp/config.json` so that project overrides do
not change which server these commands select. Choose an unused name; this
guide uses `demo`.

Save this as `demo-server.json`, replacing the URL:

```json
{
  "definition": {
    "kind": "remote",
    "url": "https://mcp.example.com/mcp",
    "transport": "streamable_http",
    "authentication": "none"
  }
}
```

For automatic OAuth client registration, change `authentication` to `oauth`
before installing. For bearer tokens or custom headers, leave it as `none` here
and configure credentials after registration.

Register without a connection check, then list the saved definition:

```sh
ripmcp install demo --config demo-server.json --skip-verify
ripmcp servers
```

The installation report identifies it as unverified. The server list includes
`demo`; its process state is `not_managed` because ripmcp does not own the remote
service. Neither command proves the endpoint is reachable.

If credentials are required, complete the appropriate
[authentication setup](authenticate.md) now. Otherwise continue.

## 2. Discover and inspect

```sh
ripmcp tools demo
```

Check that the command succeeds and its `errors` array is empty. Choose a safe tool
from the returned names. Replace `TOOL_NAME` in the following commands with that
name:

```sh
ripmcp tool demo TOOL_NAME
```

Read `tool.inputSchema` and the description. Save the required JSON object in
`arguments.json`. Use `{}` only if the schema permits no arguments. Tool detail
preserves the server's definition; ripmcp does not perform general JSON Schema
validation for you.

## 3. Invoke once and save

This step can have side effects. Run it only after checking the tool and inputs.

```sh
umask 077
ripmcp call demo TOOL_NAME --input arguments.json > result.json
status=$?
printf 'Exit status: %s\n' "$status"
```

Check the printed status before continuing. On success, `result.json` contains the
complete MCP tool-result envelope, not just text content. Exit code 7 means a
tool-reported failure whose result is still saved.
Other failures may leave an empty file or a non-result report; check the diagnostic
and [exit code](../reference/output.md#exit-codes) before interpreting the file.

## 4. Inspect the saved result

```sh
ripmcp shape result.json --depth 4 --width 20
```

This reads only the file. It does not contact `demo` or invoke the tool again.
The report shows field names, types, and array lengths. Follow the
[offline tutorial](../tutorials/inspect-json.md) to practice selecting values with `jq`.

## 5. Remove the example registration

```sh
ripmcp uninstall demo
```

This removes the local registration, not the remote service. It does not promise
credential revocation. Your argument and result files remain yours to keep or
remove.

Next: [manage tools and local processes](tools.md), or
[use project scope](projects.md).
