# Configure authentication

[Documentation](../README.md) / How-to guides

Start with an installed remote registration. If verification needs credentials
that are not configured yet, [register with `--skip-verify`](install.md).
Replace `remote`, environment names, issuers, and client IDs with your values.

## Choose a mechanism

| Server requirement                                | Use                           |
| ------------------------------------------------- | ----------------------------- |
| A token accepted in `Authorization: Bearer ...`   | Bearer setup                  |
| An API key in a custom header                     | Header setup                  |
| Browser-based remote OAuth                        | OAuth login                   |
| A local server's credential environment variables | Native local `env` references |
| A tool requests external user interaction         | `call --interactive`          |

These mechanisms are not interchangeable. A PAT works only if the MCP server
accepts it with the needed permissions. There is no universal provider app or
token conversion.

## Set a bearer token

For a hidden terminal prompt and Secret Service storage:

```sh
ripmcp auth configure remote --bearer
```

For automation, save an environment reference instead:

```sh
ripmcp auth configure remote --bearer-env SERVICE_TOKEN
```

Supply the **raw token**, without `Bearer `. The environment form saves only the
variable name; it need not be set during configuration. Supply its value through
your secret manager to each later ripmcp process. Do not put secrets in command
arguments or configuration files.

Check local availability, then verify against the endpoint through discovery:

```sh
ripmcp auth status remote
ripmcp tools remote
```

`auth configure` is offline. Its success means configuration was saved, not that
the server accepted a credential. `auth status` is also not a remote health check.

## Set a custom credential header

Choose a prompt or an environment reference:

```sh
ripmcp auth configure remote --header X-API-Key
ripmcp auth configure remote --header X-API-Key --header-env SERVICE_API_KEY
```

The value is sent unchanged. The helper rejects `Authorization` and transport-owned
headers; use bearer setup for a managed Authorization value. Use `tools remote`
to check custom-header authentication; auth status/logout do not manage arbitrary
header collections.

Each `auth configure` operation selects the requested authentication mode; it is
not a way to layer bearer authentication and OAuth. It preserves server enablement
and tool policy, but clears previous bearer/OAuth client settings as appropriate.
Changing modes does not revoke old tokens or delete potentially shared secrets.

Other custom headers remain configured and are still sent. Bearer/OAuth setup
removes an explicit `Authorization` header; custom-header setup replaces only the
named header (case-insensitively) and retains any explicit `Authorization` header.
Review `definition.headers` when changing credentials. To stop sending an obsolete
header, remove its entry from the selected configuration and renew project trust
if applicable. There is no header-removal flag.

## Use remote OAuth

For automatic client registration, import a remote definition with
`"authentication": "oauth"` and no `oauth_client`. Register with `--skip-verify`
if necessary. Then:

```sh
ripmcp auth login remote --timeout 300
ripmcp tools remote
```

Login prints instructions to stderr, opens a browser when possible, and waits
until validated credentials are securely saved. If browser launch fails, open the
printed URL manually on the same machine. Keep the command running. Ctrl-C cancels.

Automatic registration requires compatible client metadata document support or
native public dynamic registration at the issuer. If the provider requires your
own registration, register this exact callback:

```text
http://127.0.0.1:42813/oauth/callback
```

Configure a public client:

```sh
ripmcp auth configure remote \
  --oauth-client-id YOUR_CLIENT_ID \
  --issuer https://identity.example.com \
  --scope tools:read
ripmcp auth login remote
```

For a client with a secret, configure the method required by its issuer:

```sh
ripmcp auth configure remote \
  --oauth-client-id YOUR_CLIENT_ID \
  --issuer https://identity.example.com \
  --client-secret-env SERVICE_CLIENT_SECRET \
  --token-endpoint-auth-method client_secret_basic \
  --scope tools:read
```

Use `--client-secret` instead to prompt securely. With a secret, the helper's
default method is `client_secret_post`; without one it is `none`. Repeat `--scope`
for additional scopes. Use scopes actually supported by your provider.

The configured issuer must exactly match the selected issuer's metadata. A custom
registration does not bypass S256 PKCE, HTTPS, metadata, or resource validation.
See [OAuth compatibility limits](../reference/compatibility.md#authentication).

## Supply local credentials or complete tool interaction

For local servers, use `definition.env` with
[secret references](../reference/configuration.md#secret-references), then install
that server object. Local servers do not use `auth login` or `auth configure`.

If a tool uses structured URL elicitation:

```sh
ripmcp call local TOOL_NAME --input arguments.json --interactive --timeout 180
```

Read the attributed instructions on stderr and open the URL manually. ripmcp waits
for the final tool result under the original deadline. This supports structured
URL interaction, not arbitrary login text, forms, or ripmcp's own device grant.
Without `--interactive`, input-required results do not continue.

## Sign out and rotate credentials

```sh
ripmcp auth logout remote
```

- OAuth logout invalidates locally saved credentials for the selected endpoint and
  registration partition. Aliases using the same partition are affected.
- Stored bearer logout deletes the ripmcp-managed token but leaves its reference.
  Run configure again to replace it.
- Environment and external-keyring tokens remain externally managed. Logout
  exits 10 without a success report; rotate or remove them in their source.
- Logout does not revoke tokens at the provider or undo requests already sent.

For environment references, change the variable value supplied to the next
invocation. For stored values, run the relevant configure command again.

## Storage and project requirements

Secret Service operations require a session bus and an existing unlocked default
collection. There is no plaintext fallback or automatic unlock. Environment-only
setup does not require a keyring. Hidden prompts use `/dev/tty`; tokens exceeding
4094 bytes should use environment references rather than terminal input.

`auth configure` defaults to user scope. If a project overrides that server, use
`--project` for the project target or run outside the project for the user target.
Project mutations require renewed `ripmcp trust`. Login/status/logout use the
effective definition and do not accept scope flags.

Bearer status and logout also require the server to be enabled. If it is disabled,
enable it in the correct scope first and renew project trust if needed; enabling
does not contact the server. OAuth status/logout can inspect or clear a disabled
registration, but still require project trust.
