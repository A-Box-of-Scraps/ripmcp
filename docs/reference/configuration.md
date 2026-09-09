# Configuration reference

[Documentation](../README.md) / Reference

## Locations

| Purpose             | Path                                     | Default when applicable               |
| ------------------- | ---------------------------------------- | ------------------------------------- |
| Project config      | Nearest ancestor `.ripmcp/config.json`   | No implicit creation                  |
| User config         | `$XDG_CONFIG_HOME/ripmcp/config.json`    | `~/.config/ripmcp/config.json`        |
| Managed data        | `$XDG_DATA_HOME/ripmcp/`                 | `~/.local/share/ripmcp/`              |
| State and ownership | `$XDG_STATE_HOME/ripmcp/`                | `~/.local/state/ripmcp/`              |
| Cache               | `$XDG_CACHE_HOME/ripmcp/`                | `~/.cache/ripmcp/`                    |
| Runtime socket area | `$XDG_RUNTIME_DIR/ripmcp/`               | Validated `/tmp/ripmcp-UID/` fallback |
| Credential values   | Secret Service or referenced environment | No plaintext-file fallback            |

XDG overrides must be absolute and contain no parent-directory components.
Invalid config/data/state/cache overrides use the HOME defaults. The runtime base
must also pass private ownership and permission checks; its fallback ignores
`TMPDIR`. Unsafe existing ripmcp state is rejected, not silently repaired.
Storage is created when needed, not all at startup. Sensitive state directories
and files require the effective user and modes 0700/0600 respectively.

Trust approvals are stored in state `trust.json`, outside the selected project.
`ownership.json`, `artifacts.json`, and retry metadata are generated internal state,
not configuration to copy between projects. OAuth lock/epoch files use the private
`/tmp/ripmcp-UID/` area independently of XDG overrides; they contain no tokens.

## Configuration document

User and project configuration share this structure:

```json
{
  "schema_version": 1,
  "servers": {},
  "timeouts": {
    "operation_seconds": 60,
    "login_seconds": 300
  }
}
```

| Field               | Requirement                                                            |
| ------------------- | ---------------------------------------------------------------------- |
| `schema_version`    | Required; integer 1                                                    |
| `servers`           | Map from server name to server object; default empty                   |
| `timeouts`          | Optional non-null object; omitted members take defaults                |
| `operation_seconds` | Positive unsigned 64-bit integer; default 60                           |
| `login_seconds`     | Positive unsigned 64-bit integer; default 300; also used for configure |

Unknown fields, duplicate map keys, trailing documents, malformed selected
configuration, and unsupported versions fail. Server names and disabled tool names
must be nonempty and contain no control characters. Stored configuration reads
are bounded to 4 MiB.

## Server object

This is the object accepted by `install SERVER --config FILE`, without a surrounding
`schema_version` or `servers` map:

```json
{
  "enabled": true,
  "disabled_tools": [],
  "definition": {
    "kind": "remote",
    "url": "https://mcp.example.com/mcp",
    "transport": "streamable_http",
    "authentication": "none"
  }
}
```

`enabled` defaults to true, `disabled_tools` to an empty set. `definition` is
required. New tools are allowed unless their names are disabled; disabling the
server overrides all tool settings.

### Local definition

Example import file, with package/path values to replace:

```json
{
  "definition": {
    "kind": "local",
    "runtime": "npx",
    "package": "@example/mcp-server@1.2.3",
    "transport": "stdio",
    "args": [],
    "env": {
      "API_TOKEN": { "env": "SERVICE_TOKEN" }
    },
    "cwd": "/absolute/server/workspace"
  }
}
```

| Field       | Meaning                                                                            |
| ----------- | ---------------------------------------------------------------------------------- |
| `runtime`   | Required: `npx`, `uvx`, or `docker`                                                |
| `package`   | Required package specification or image; must match a prepared installation to run |
| `transport` | Required: `stdio`                                                                  |
| `args`      | Literal argument array; default empty                                              |
| `env`       | Child variable name to secret reference; default empty                             |
| `cwd`       | Optional absolute working directory                                                |

Default cwd is `/` for user definitions and the canonical project root for project
definitions. It is not the invoking shell's current directory for user servers.

Local child processes receive a limited base environment: `PATH`, `HOME`, `LANG`,
`LC_ALL`, and the five XDG variables in the locations table, plus explicitly mapped
`env` references. Other shell variables are not inherited automatically. The
referenced value is resolved from the caller on use, not frozen at daemon startup.
For Docker, explicit environment names are forwarded into the container; `cwd` is
the host launch directory, not a container workdir option.

Install these definitions through `install` so preparation and immutable runtime
resolution are recorded. Adding JSON alone does not prepare a local server.

### Remote definition

| Field            | Meaning                                                          |
| ---------------- | ---------------------------------------------------------------- |
| `url`            | Required HTTP(S) MCP endpoint, without userinfo or fragment      |
| `transport`      | Required: `streamable_http`                                      |
| `headers`        | Header name to secret reference; default empty                   |
| `authentication` | `none` (default), `oauth`, or `bearer`                           |
| `bearer`         | Required reference only when authentication is `bearer`          |
| `oauth_client`   | Optional client registration only when authentication is `oauth` |

Operational HTTP endpoints require HTTPS except unauthenticated loopback HTTP.
Configured credential headers or authentication require HTTPS. OAuth MCP URLs must
be query-free. Transport-owned headers and case-insensitive duplicates are rejected.

For bearer authentication, the referenced value is a raw token; ripmcp adds the
prefix. An explicit native `Authorization` header is allowed only with
`authentication: "none"`, and its value must contain the complete header value.
The configure helper does not create that legacy form. OAuth and managed bearer
cannot be combined with an explicit Authorization header.

### OAuth client object

```json
{
  "issuer": "https://identity.example.com",
  "client_id": "YOUR_CLIENT_ID",
  "client_secret": { "env": "SERVICE_CLIENT_SECRET" },
  "token_endpoint_auth_method": "client_secret_post",
  "scopes": ["tools:read"]
}
```

`issuer` and `client_id` are required. `client_secret` is optional.
`token_endpoint_auth_method` defaults to `none` in native JSON; unlike the CLI
helper, native JSON does not infer Post from the presence of a secret. Set the
method explicitly when supplying a secret. `scopes` defaults to an empty array.

Methods are `none`, `client_secret_post`, and `client_secret_basic`. None requires
no secret; the other methods require one. The issuer is query-free HTTPS and must
exactly match selected metadata. Limits: issuer 8192 bytes, nonempty client ID
4096 bytes, at most 128 scopes and 8192 aggregate scope bytes. Scope tokens must
be nonempty printable OAuth scope tokens, without spaces, quotes, or backslashes.

Omit `oauth_client` to use automatic registration. See
[authentication setup](../how-to/authenticate.md#use-remote-oauth).

## Secret references

Each reference is exactly one of:

| Form                               | Source                                                                                                      |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `{"env":"SERVICE_TOKEN"}`          | Environment variable in the invoking process                                                                |
| `{"keyring":"external-id"}`        | Externally provisioned Secret Service reference                                                             |
| `{"stored":"64-lowercase-hex-id"}` | ripmcp-managed secret generated by configure; notation shows the required ID shape, not a usable literal ID |

Do not put raw secret strings in these fields. Environment names match
`[A-Za-z_][A-Za-z0-9_]*`. External keyring IDs use nonempty ASCII letters, digits,
underscore, dot, and hyphen, excluding `.` and `..`. Stored IDs have exactly 64
lowercase hex characters; do not manufacture or reuse them as authority.

External keyring references use attributes `application=ripmcp`,
`namespace=references-v1`, and `identity=ID`. They are separate from OAuth records.
Configure-generated values are managed by ripmcp and tracked for self-removal.
An endpoint or registration change cannot use a nickname to inherit unrelated
OAuth credentials.

## Selection and writes

1. Read user configuration.
2. Canonicalize cwd and select the nearest ancestor `.ripmcp/config.json`, up to
   filesystem root. Do not combine nested projects.
3. Replace each same-named user server with the complete project server object.
4. Require content/location-bound trust before using project definitions. Do not
   fall back to a user definition hidden by an untrusted project.
5. Replace the entire timeout section when an approved project supplies one;
   omitted members use built-in defaults, not the user's corresponding value.
6. Apply a CLI timeout override if supplied.

Mutation scope defaults to user. `--project` needs a discovered, trusted selection.
Changed project bytes invalidate approval, including CLI-generated changes.
Semantic no-op mutations preserve unchanged bytes and trust. Read/call commands
have no scope override. See the [project procedure](../how-to/projects.md).

## Implementation source

[Schema](../../src/config/schema.rs), [auth schema](../../src/config/authentication.rs),
[effective selection](../../src/config/effective.rs), [paths](../../src/storage/paths.rs),
[local launch](../../src/supervisor/launch.rs), and [config tests](../../tests/config.rs).
