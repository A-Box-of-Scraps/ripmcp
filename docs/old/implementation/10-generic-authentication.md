# Generic authentication

Implemented September 9, 2026. This extends the phase 05 authentication contract;
it does not claim compatibility with every OAuth provider or every type of token.

## Supported mechanisms

| Mechanism                                   | Support                                                                                           |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| OAuth client metadata documents             | Existing automatic registration                                                                   |
| Native public dynamic client registration   | Existing automatic registration                                                                   |
| User-supplied OAuth client registration     | Authorization code with S256 PKCE; public clients, `client_secret_post`, or `client_secret_basic` |
| PAT or API token accepted as a bearer token | Raw token from an environment variable or ripmcp-managed Secret Service storage                   |
| Custom API-key HTTP header                  | Environment, external keyring, or ripmcp-managed stored reference                                 |
| Local MCP server-managed authorization      | `call --interactive` URL elicitation; separate from remote OAuth                                  |

Device authorization, client-credentials grants, implicit/password grants,
`private_key_jwt`, mutual TLS, private-network OAuth providers, arbitrary callback
URLs, and provider-specific login protocols are not implemented. A PAT is usable
only if the target server accepts that token and its permissions. Configuration
does not remove provider restrictions, organization approval, or expiration.

ripmcp does not ship a registered OAuth app for each provider. Users or their
organizations register clients when automatic registration is unavailable. One
provider's app is not a universal registration for unrelated providers.

## Bearer setup

Configure an existing remote server:

```sh
ripmcp auth configure github --bearer
```

The command checks the target and secure storage, then reads the raw token from
`/dev/tty` with terminal echo disabled. No token argument or plaintext configuration
value is accepted. The terminal settings are restored on completion, timeout,
error, or Ctrl-C. The prompt accepts up to 4094 bytes; longer tokens should use an
environment reference to avoid the Linux canonical-input limit.

For automation:

```sh
ripmcp auth configure github --bearer-env GITHUB_TOKEN
```

The variable contains the **raw token**, not `Bearer <token>`. ripmcp adds the
prefix when constructing the Authorization header. Setup stores only the variable
name and does not require the variable to be set. Set it in each invoking process's
environment using your normal secret-management mechanism. It is resolved again
on subsequent invocations, so rotating the environment value does not require
rewriting the configuration.

For a first installation, start with a remote server-definition JSON file:

```json
{
  "definition": {
    "kind": "remote",
    "url": "https://api.githubcopilot.com/mcp/",
    "transport": "streamable_http",
    "authentication": "none"
  }
}
```

```sh
ripmcp install github --config github.json --skip-verify
ripmcp auth configure github --bearer-env GITHUB_TOKEN
ripmcp tools github
```

`tools` verifies the connection through discovery without invoking a tool.
`auth configure` itself is offline: success means configuration was saved, not
that the provider accepted the token. It preserves enablement and tool policy.

## Custom credential headers

```sh
ripmcp auth configure service --header X-API-Key
ripmcp auth configure service --header X-API-Key --header-env SERVICE_API_KEY
```

The first form prompts and stores the value securely. The second records an
environment reference. The value is sent unchanged; no Bearer prefix is added.
Transport-owned headers and Authorization are rejected by these helper forms.
Use `--bearer` or `--bearer-env` for managed bearer authentication. Existing native
JSON configurations with an explicit Authorization header remain supported and
must supply the complete header value.

## User-supplied OAuth registration

Public client:

```sh
ripmcp auth configure service \
  --oauth-client-id my-client-id \
  --issuer https://identity.example.com \
  --scope tools:read
ripmcp auth login service
```

Client with a secret:

```sh
ripmcp auth configure service \
  --oauth-client-id my-client-id \
  --issuer https://identity.example.com \
  --client-secret-env SERVICE_CLIENT_SECRET \
  --token-endpoint-auth-method client_secret_basic \
  --scope tools:read
```

`--client-secret` instead prompts and saves the secret in secure storage. Client
IDs are public configuration; client secrets are always references. Without a
secret the default authentication method is `none`; with a secret it is
`client_secret_post`. Use the method required by the provider. Repeat `--scope`
for multiple scopes. With explicit scopes, ripmcp does not automatically request
every scope advertised by the resource; required challenge scopes can still be
added. Scope and secret/method combinations are validated before configuration
changes.

Register this exact callback with the provider:

```text
http://127.0.0.1:42813/oauth/callback
```

The issuer is required, not guessed from an arbitrary MCP challenge when sending
client credentials. Its value must exactly match the selected authorization
server's metadata issuer. Provider metadata still must advertise a compatible
authorization-code flow and S256 PKCE. OAuth destinations must be public HTTPS;
the MCP OAuth endpoint and issuer must be query-free. The loopback callback is
the local exception, not permission to send secrets to insecure remote endpoints.

The configured client takes precedence over automatic registration. Secrets are
sent only to the token endpoint from validated issuer metadata, never in the
browser URL or MCP Authorization header. Both code exchange and refresh use the
configured token-endpoint authentication method. Basic authentication applies
form encoding to the client ID and secret before Base64 encoding. Redirects and
automatic retries of token exchanges remain disabled.

## Native configuration

Bearer example:

```json
{
  "definition": {
    "kind": "remote",
    "url": "https://example.com/mcp",
    "transport": "streamable_http",
    "authentication": "bearer",
    "bearer": { "env": "SERVICE_TOKEN" }
  }
}
```

Pre-registered OAuth example:

```json
{
  "definition": {
    "kind": "remote",
    "url": "https://example.com/mcp",
    "transport": "streamable_http",
    "authentication": "oauth",
    "oauth_client": {
      "issuer": "https://identity.example.com",
      "client_id": "my-client-id",
      "client_secret": { "env": "SERVICE_CLIENT_SECRET" },
      "token_endpoint_auth_method": "client_secret_post",
      "scopes": ["tools:read"]
    }
  }
}
```

Omitting `oauth_client` retains automatic registration. Existing `none` and
`oauth` values remain valid. `bearer` requires a bearer reference and cannot be
combined with a configured Authorization header. `oauth_client` is valid only
with `authentication: "oauth"`.

Secret references support `{"env":"NAME"}`, external
`{"keyring":"opaque-id"}`, and ripmcp-owned `{"stored":"64-lowercase-hex-id"}`.
The stored form is generated by the setup command; users should not manufacture
IDs. It refers to a JSON string in ripmcp's Secret Service namespace, not a
plaintext file. External keyring references retain their existing separate
namespace. The new references also work in native local-server environment
configuration; the configure helper itself targets remote servers only.

## Lifecycle, ownership, and safety

- `auth configure` defaults to user scope. `--project` requires an already trusted
  selected project. Shadowed targets are rejected rather than silently modifying
  a different server. Project mutations require renewed `ripmcp trust` approval.
- A single timeout includes setup and user input; configure defaults to
  `timeouts.login_seconds` and accepts the global `--timeout` override.
- Token setup requires HTTPS. Secrets are never written to config, diagnostics,
  ordinary logs, or process arguments. Secret Service unavailability fails closed;
  environment references need no keyring.
- `auth status` for bearer authentication checks local credential availability and
  syntax, not provider acceptance. `auth login` is not applicable to bearer tokens.
- `auth logout` deletes a ripmcp-stored bearer token but leaves its reference in
  config; configure a replacement to use it again. It does not revoke the provider's
  token. Environment and external-keyring credentials are not deleted; the command
  directs users to manage those externally. Custom-header configurations continue
  to use `tools` for verification; OAuth/bearer status and logout do not manage
  arbitrary header collections.
- Pre-registered OAuth caches are partitioned by MCP endpoint and registration
  configuration. Changing client ID, issuer, secret reference, method, or scopes
  cannot reuse another configuration's saved tokens. OAuth status and logout do
  not require resolving the client secret. They operate on that partition.
- Switching authentication modes does not revoke old provider tokens or delete
  potentially shared credentials. New stored values use unique IDs. Interrupted
  setup may leave an unreferenced secret; all ripmcp-created keys are tracked for
  self-uninstall cleanup. There is no rollback to plaintext storage.
- Setup compares the selected configuration again before committing. Credentials
  are durably stored and verified before their reference is published. Trust,
  policy, endpoint binding, refresh invalidation, and cancellation protections from
  phase 05 remain in effect.

## Validation

Coverage includes offline environment setup, runtime rotation, missing/invalid
tokens, reserved headers, configuration preservation on failure, untrusted project
overrides, native client configuration validation, issuer mismatch, authentication
method mismatch, public/Post/Basic exchanges and refresh, S256 PKCE, cache
partitioning, and hidden terminal input with restoration after success, timeout,
and cancellation. No test uses a real provider credential.
