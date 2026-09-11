# Compatibility and validation limits

[Documentation](../README.md) / Reference

This page describes the profile implemented in this checkout. It does not claim
that a named public provider or arbitrary MCP package has been tested successfully.

## Platform and MCP

| Area             | Implemented profile                                                            |
| ---------------- | ------------------------------------------------------------------------------ |
| Platform         | Linux; managed processes require kernel pidfd support and private Unix sockets |
| Protocol version | Exact `2026-07-28`; no implicit older-version fallback                         |
| Local transport  | stdio through prepared npx, uvx, or Docker installations                       |
| Remote transport | HTTP/1.1 Streamable HTTP POST with JSON or request-scoped SSE replies          |
| Discovery        | `server/discover`, followed by paginated `tools/list`                          |
| Calls            | `tools/call`, full result envelope, no automatic replay                        |
| User interaction | Explicit `--interactive` structured URL elicitation continuations              |
| HTTP security    | Verified TLS; HTTPS except unauthenticated loopback HTTP                       |

The client sends the exact protocol revision and client identity in request
metadata. It uses `server/discover`, not legacy `initialize` or an initialized
notification. The server must advertise the target revision and tools capability.
HTTP tool-header annotations are validated; invalid definitions can make discovery
partial.

This profile does not use persistent HTTP sessions, GET/DELETE session endpoints,
stream resumption, redirects, automatic proxy discovery, or automatic retries.
Server-originated JSON-RPC requests fail the transport. Raw local server stderr
is drained but not retained as readable logs; only byte/chunk counters are kept.

Not implemented: resource/prompt workflows, sampling, roots, form elicitation,
subscriptions, task workflows, general result querying, registry browsing,
automatic package updates, or idle shutdown. Merely being an MCP server does not
establish compatibility with this exact profile.

## Authentication

Supported remote mechanisms:

- Raw bearer tokens via environment, external keyring, or ripmcp-managed storage.
- Custom credential headers using secret references.
- OAuth authorization code with S256 PKCE, resource/issuer validation, and refresh
  where supported.
- Automatic client metadata document identity, or native public dynamic client
  registration as a compatibility fallback.
- Explicit issuer-bound registrations with `none`, `client_secret_post`, or
  `client_secret_basic` token-endpoint authentication.

OAuth uses the exact local callback `http://127.0.0.1:42813/oauth/callback`.
The automatic hosted client identity is configured as:

```text
https://a-box-of-scraps.github.io/ripmcp/oauth/client.json
```

This is a configured identity, not a live availability check. Its source is
[the hosted metadata document](../../site/oauth/client.json).

OAuth HTTP destinations must be public HTTPS. Literal private addresses and DNS
answers containing non-public addresses are rejected. This restriction is separate
from the unauthenticated loopback allowance for the MCP transport. The local
callback is the intended HTTP exception, not an insecure provider fallback.

OAuth credentials and prompted secrets require an existing unlocked default
Secret Service collection with an encrypted session. No plaintext storage,
automatic unlock, or fallback collection is provided. Environment-only bearer or
header configuration does not need a keyring.

Not implemented by ripmcp's OAuth client: device grant, client-credentials grant,
implicit/password grants, `private_key_jwt`, mutual TLS, private-network issuers,
arbitrary callback URLs, or remote-headless redirects. A server-managed URL
interaction can show a device code, but that does not add a device grant to
`auth login`.

## Validation status

The repository has CLI, local/HTTP transport, scoped trust, authentication-seam,
lifecycle, policy, shape, and sandboxed cleanup tests. Default installation tests
use fake runtime executables; OAuth tests use isolated fixtures and fake-store
seams rather than personal credentials.

The current [test coverage evidence](../../tests/validation/coverage.md#remaining-integration-gaps)
records two missing full-system checks. **v1 validation remains incomplete:**

1. One authenticated end-to-end CLI lifecycle combining production TLS and an
   isolated real Secret Service adapter, login, call, refresh, logout, and project
   endpoint override/trust renewal.
2. Measured peak-memory/error stress evidence for concurrent maximum-size
   supervisor replies.

Separate component tests are not a substitute for those missing checks. The
optional real-runtime smoke test also requires deliberate package/runtime
selection; an ignored test is not a compatibility pass. Do not treat this
documentation as a declaration that all v1 readiness gates are complete.

## Implementation source

[Protocol constants and limits](../../src/mcp/mod.rs),
[wire protocol](../../src/mcp/protocol.rs), [HTTP transport](../../src/mcp/http.rs),
[interaction](../../src/mcp/interaction.rs), [auth provider](../../src/auth/provider.rs),
and [coverage evidence](../../tests/validation/coverage.md).
