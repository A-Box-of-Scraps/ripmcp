# Use project-specific servers

[Documentation](../README.md) / How-to guides

## Create the project configuration

From the intended project root, create `.ripmcp/config.json` with:

```json
{
  "schema_version": 1,
  "servers": {}
}
```

Create it only if it does not already exist; do not overwrite another configuration.
A `.ripmcp` directory alone does not select a project. The CLI does not invent a
project root for `--project`.

Read the file and approve it in a terminal:

```sh
ripmcp trust
```

Trust previews execution and endpoint effects with sensitive details redacted.
Review the actual file as well. Answer `y` only for content you intend to permit.
There is no `trust -y`; both stdin and stderr must be terminals.

## Install into the project

With a prepared native server input file such as the one in the
[installation guide](install.md):

```sh
ripmcp install remote --project --config remote.json --skip-verify
ripmcp trust
```

Installation changes the project configuration. The second approval covers those
new bytes. The same rule applies after `enable`, `disable`, `auth configure`,
`uninstall`, or a manual edit that changes the file.

For a bearer environment reference:

```sh
ripmcp auth configure remote --project --bearer-env SERVICE_TOKEN
ripmcp trust
ripmcp tools remote
```

The secret value remains outside the project. Supply it to the invoking process.

## Check which definition wins

```sh
ripmcp servers
```

Inspect `provenance` and `trust_required`. ripmcp selects the nearest ancestor
`.ripmcp/config.json`, searching through the filesystem root. Nested projects are
not combined. A selected malformed configuration is an error.

A project server replaces the entire same-named user server, including command,
credentials, and policy. An untrusted project override blocks that name; ripmcp
does not fall back to the user definition.

Read and call commands use this effective selection. Scope flags select a **write
target**, not a temporary read override. To inspect or call a shadowed user server,
run outside the overriding project.

## Change policy in the right scope

```sh
ripmcp disable remote TOOL_NAME --project
ripmcp trust
```

Omitting `--project` targets user configuration, even when run inside a project.
Mutations can report `shadowed_by_trusted_project` and
`project_reapproval_required`. Authentication setup rejects a shadowed target;
tool policy changes that require discovery also cannot inspect a shadowed user
definition.

Trust binds the canonical project location and exact file bytes. Moving a project
or editing its content requires approval again. Trust records stay outside the
repository. Do not commit generated state or credential values.

Details: [configuration precedence](../reference/configuration.md#selection-and-writes)
and [the trust model](../explanation/lifecycle-and-safety.md#trust-is-not-a-sandbox).
