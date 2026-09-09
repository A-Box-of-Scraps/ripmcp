# 05: OAuth login and credential lifecycle - 2026-09-08

Status: **Done**

Dependencies: 02, 03.

Contract gates: D05, D08; approved client-registration and secure-storage profile.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/auth/ (new or modify when introduced by an earlier phase)
+ src/mcp/ (new or modify when introduced by an earlier phase)
+ tests/auth.rs (new or modify when introduced by an earlier phase)
+ tests/support/ (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Verify the exact authorization profile and choose supported metadata discovery/client registration paths. Define issuer/resource/client identity, provider incompatibility errors and the secure credential backend. Keep local stdio environment credentials separate.
2. [x] Implement metadata and issuer validation, resource binding, PKCE and response validation required by the approved profile. Restrict redirects and credential transmission to validated destinations. Bind stored tokens to actual resource and issuer, not server nickname.
3. [x] Implement `auth login <server>` with a loopback callback listener, unpredictable state, browser launch, immediate stderr instructions, bounded timeout and cancellation. Validate callback state and response before token exchange; return success only after secure persistence.
4. [x] Implement the approved browser-unavailable path and clear unsupported headless/provider errors. Reject login for unsupported transport/configuration before side effects. Do not weaken callback or TLS validation as a fallback.
5. [x] Implement token lookup, expiry handling and refresh when supported, with synchronization and atomic token replacement. Require explicit login when refresh fails permanently. Integrate remote verification/discovery/calls without implicit interactive login or replay of uncertain tool calls.
6. [x] Implement `auth status` without displaying tokens and `auth logout` removing locally saved credentials for the bound identity. Invalidate in-memory credentials across relevant processes. Define handling when multiple configured names refer to the same identity.
7. [x] Test secret-store failures, denied consent, callback replay/mismatch, timeouts, cancellation, refresh races, changed project endpoints and malicious metadata/redirects. Use local fixtures and a fake secure-store adapter for default tests.

## Acceptance criteria

- Login blocks until validated credentials are stored; failure never reports success.
- Ordinary discovery, installation verification and invocation never wait for browser interaction.
- Project endpoint overrides cannot receive credentials bound to another resource/issuer.
- Logout prevents subsequent use of cached credentials; status and logs contain no secrets.
- Provider/profile limitations are explicit; repository validation gate passes.

## References

authentication.md; configuration.md; v1-scope.md protocol target.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: Steps 1-7 and the acceptance criteria. Explicit login,
  status/logout, secure persistence, refresh and qualified remote operations are
  implemented. Local keyring references use a separate namespace and asynchronous,
  deadline-bounded resolution.
- In-progress task: None. Installation registration/verification commands and
  cross-server/shorthand workflows remain phases 06 and 07 respectively.
- Changed paths / commits: `Cargo.toml`, `Cargo.lock`, `src/auth/`, `src/lib.rs`,
  `src/mcp/http.rs`, `src/trust/secrets.rs`, supervisor integration, `tests/auth.rs`,
  `tests/cli.rs`, `tests/lifecycle/ipc.rs`, and implementation trackers/contracts.
  The user subsequently authorized committing and pushing this phase; see Git
  history for its commit. The earlier Pages-only commit is `5b518ca`.
- Tests run and results, September 8, 2026:
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: passed, 183 tests.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
  - `cargo test -q --lib auth::` and `cargo test -q --test auth`: passed on five
    consecutive runs before the final report-snapshot addition; the final full
    suite includes that addition.
  - Full validation exposed a supervisor startup publication race: a client could
    read no record, then see the just-published socket and reject it as unsafe.
    Socket visibility is now sampled before reading the record, preserving all
    ownership/nonce/peer checks and malformed-record errors. A repeated-bootstrap
    regression test was added; the subsequent complete gate passed.
  - After that fix, `cargo test -q` passed three further consecutive full-suite
    runs, 183 tests per run. README and Pages files are unchanged.
- Decisions applied: D05/D08, the approved hosted client identity, S256 PKCE,
  explicit browser login/manual URL fallback, and fail-closed Secret Service
  storage. No device flow, implicit browser login, plaintext fallback or older
  MCP revision was introduced.
- Blockers / remaining questions: None for this phase. Real-provider and personal
  keyring smoke tests were intentionally not run. Default tests use isolated
  loopback fixtures and an injected fake secure store, not personal credentials.
- Next action: Implement phase 06 using the authorized remote HTTP options factory
  and `mcp::Client`; installation verification must never call the login flow.
  Commit/push authorization now includes this completed phase.

## Verified authorization profile

The exact MCP `2026-07-28` authorization pages were rechecked, including resource
and issuer discovery, registration, PKCE and security considerations. The client
uses its existing raw-JSON MCP transport rather than assuming SDK OAuth defaults.

- Each OAuth endpoint is an explicit, query-free HTTPS MCP URL. Credential lookup
  is indexed by that exact parsed endpoint; the saved payload additionally binds
  canonical resource, exact issuer, client ID and token endpoint. A lookup key or
  nickname is never authority to transmit a token. Origin-root resource IDs omit
  the trailing slash; other paths must match exactly. Broader resource metadata
  identifiers are not accepted as implicit authority for another endpoint.
- Login makes an unauthenticated `server/discover` probe. Bearer challenges are
  parsed across multiple authentication schemes/header fields. A supplied
  `resource_metadata` URL takes precedence; otherwise discovery tries the
  path-specific and root protected-resource well-known locations. The resource
  must bind the requested endpoint. The first advertised issuer is selected
  deterministically; failure never switches to a different issuer.
- Issuer discovery uses RFC 8414 followed by the two OIDC path arrangements where
  applicable. Only missing/unsupported locations advance discovery; malformed,
  redirected or mismatched metadata fails closed. Issuer comparison is exact.
  S256 support, authorization-code response type and public-client authentication
  support are checked. Unknown document extensions are tolerated; duplicate JSON
  keys and responses exceeding 256 KiB are rejected.
- Hosted Client ID Metadata Documents are preferred when advertised. Otherwise,
  optional DCR requests a native public client, fixed redirect, authorization-code
  and refresh grants, and `token_endpoint_auth_method: none`. Confidential clients
  and modified redirect registrations are rejected. DCR remains a deprecated,
  optional compatibility fallback. Pre-registration-only providers are explicitly
  unsupported; no pre-registered client configuration was added silently.
- HTTP redirects, proxy autodiscovery, Referer propagation, connection pooling and
  request retries are disabled for OAuth. TLS verification remains enabled. OAuth
  HTTP destinations must be public: literal private addresses and DNS answers
  containing non-public addresses are rejected by the actual connector resolver.
  Private-network OAuth providers are an explicit v1 limitation. The loopback HTTP
  fixture exception exists only in test builds, never behind a production flag.
- Scope selection prefers the challenge over `scopes_supported`, without requiring
  the former to be a subset. Explicit reauthorization unions prior granted scopes
  and pending scope challenges. Ordinary 401/403 handling never replays the MCP
  request: it records login-required state and any scope request only when the
  sent token still matches the saved credential.

## Login and persistence lifecycle

- The client identity is the published document:
  `https://a-box-of-scraps.github.io/ripmcp/oauth/client.json`.
  The exact callback is `http://127.0.0.1:42813/oauth/callback`. An occupied port
  fails explicitly; there is no alternate redirect, public listener or device
  flow. Tests assert these constants match the Pages source document.
- Login verifies configuration/trust and secure-store availability before binding
  the listener. State and PKCE verifier each use 256 bits of OS randomness. The
  authorization URL includes S256, resource and the exact redirect. Instructions
  are printed immediately to stderr; browser-opening failure leaves manual local
  completion available. The configured login deadline covers the whole flow.
- The callback validates method, host, bounded HTTP framing, unique query fields,
  state, issuer when supplied/advertised, and success versus denial. No code is
  exchanged before validation. The listener is dropped on success, cancellation
  or timeout. A new login has new state; old callbacks cannot complete it.
- Trust/configuration is rechecked after user interaction, before exchange and
  before persistence. A generation check prevents a pending login from restoring
  credentials after logout or another credential operation. Success is returned
  only after secure write and read-back verification.
- The concrete backend is an existing unlocked default Secret Service collection
  on the local Unix session bus. `secret-service` 5.2.0 with Rust cryptography and
  Tokio uses the encrypted DH session. No plain-session, file, session-collection,
  arbitrary-collection or keyring-unlock fallback is used. Missing/locked storage
  fails closed. Creation is confined to explicit login; noninteractive operations
  only read or replace existing items and do not execute unlock/create prompts.
- Secret Service OAuth items use attributes `application=ripmcp`,
  `namespace=oauth-v1`, `identity=<endpoint SHA-256>`. Generated credentials stay
  outside config. Generic `{"keyring":"id"}` references use the distinct
  `references-v1` namespace and their opaque ID as `identity`, preventing a
  project reference from extracting an OAuth credential record. Environment
  references remain separate. Neither generic nor OAuth secret values are logged.

## Refresh, logout and integration contracts

- Every request reads the secure store anew and validates fresh resource/issuer
  metadata before returning authorization. No bearer-token cache lives in the
  CLI or supervisor. Identical effective endpoints share credentials across
  configured names/scopes, but each operation must independently pass trust and
  policy checks. Changed project endpoints never inherit another endpoint's token.
- Positive token lifetimes are stored as absolute expiry, with a 30-second refresh
  margin and a clock-rollback check. Omitted expiry remains unknown until a server
  challenge. Refresh sends the resource and public client identity. Returned
  refresh tokens must rotate if present; omission ends future refresh capability.
- Per-endpoint OS file locks in validated `/tmp/ripmcp-<uid>` serialize credential
  mutations across processes and XDG settings. Login does not hold the lock while
  waiting for browser consent. Refresh invalidates the old generation before
  sending a refresh token. An uncertain exchange is never repeated automatically;
  failed exchange/persistence requires explicit login.
- Non-secret, atomically replaced epoch files independently invalidate delayed
  D-Bus writes. A timed-out or cancelled store operation cannot later resurrect a
  credential after logout. The only filesystem auth data is locks/random epochs,
  not tokens, authorization codes, PKCE verifiers or registration secrets.
- Logout replaces the credential payload with an empty generation tombstone and
  verifies the replacement; the keyring item's non-secret attributes may remain.
  This removes all locally saved tokens for the effective endpoint, including
  shared aliases. It does not revoke tokens at the provider or roll back requests
  already authorized/in flight. Corrupt/unavailable stores fail explicitly.
- `auth status` is local and noninteractive. Its versioned JSON has only server,
  state (`saved`, `signed_out`, `expired`, `login_required`),
  `shared_by_endpoint: true` and `remote_validity_checked: false`. Saved does not
  claim current provider acceptance. Reporting snapshots contain no credentials.
- `auth::remote::options` constructs noninteractive authenticated HTTP options from
  an authorized effective server. Qualified remote tools/tool/call use this path;
  calls preserve complete result envelopes and never replay uncertain delivery.
  The HTTP challenge hook receives the authorization actually sent, preventing a
  late rejection from blindly replacing a different saved token.
- Local preparation resolves generic keyring references asynchronously under the
  operation deadline and rechecks the effective snapshot after the wait. The
  supervisor inherits only the session-bus address for this backend, not an OAuth
  token snapshot; child environments still follow their separate explicit policy.
  A supervisor started without a session bus must be restarted to acquire one.

## Audit and hosting evidence

Primary references:

- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration`
- `https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations`
- `https://specifications.freedesktop.org/secret-service/latest/`
- `https://docs.rs/secret-service/5.2.0/secret_service/`

The concrete dependency source audit covered `SecretService::connect`, default
collection lookup, item search, locked checks, encrypted get/set, and creation
prompt behavior. Ordinary reads/set-secret do not call unlock or prompt methods.
Cargo.lock pins secret-service 5.2.0, zbus 5.19.0 and getrandom 0.3.4; existing
reqwest/Tokio/URL/SHA-256 implementations remain the transport primitives.

The user selected project-hosted metadata and authorized Pages-only commits on
`main`. Pages creation initially failed because the repository was private on an
unsupported plan. The user made it public, and commit `5b518ca` published only
`.github/workflows/pages.yml` and `site/oauth/client.json`. Workflow run
`34280164525` passed; HTTPS returned 200, application/json, no redirect and exact
source bytes with matching client_id. HTTPS enforcement was enabled. Source
changes were initially staged only; the user subsequently authorized commit/push.

## Generic authentication extension

The original native-public-registration-only scope is superseded by
[generic authentication](10-generic-authentication.md) for explicitly configured
clients. Public, Basic, and Post token-endpoint authentication are now supported
with pinned issuers and isolated credential caches. Bearer and custom-header
setup use secret references. Device flow and provider-specific authentication
extensions remain unsupported by ripmcp's remote OAuth client.
