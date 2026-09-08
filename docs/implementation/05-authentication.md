# 05: OAuth login and credential lifecycle - 2026-09-08

Status: **Not started**

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

1. [ ] Verify the exact authorization profile and choose supported metadata discovery/client registration paths. Define issuer/resource/client identity, provider incompatibility errors and the secure credential backend. Keep local stdio environment credentials separate.
2. [ ] Implement metadata and issuer validation, resource binding, PKCE and response validation required by the approved profile. Restrict redirects and credential transmission to validated destinations. Bind stored tokens to actual resource and issuer, not server nickname.
3. [ ] Implement `auth login <server>` with a loopback callback listener, unpredictable state, browser launch, immediate stderr instructions, bounded timeout and cancellation. Validate callback state and response before token exchange; return success only after secure persistence.
4. [ ] Implement the approved browser-unavailable path and clear unsupported headless/provider errors. Reject login for unsupported transport/configuration before side effects. Do not weaken callback or TLS validation as a fallback.
5. [ ] Implement token lookup, expiry handling and refresh when supported, with synchronization and atomic token replacement. Require explicit login when refresh fails permanently. Integrate remote verification/discovery/calls without implicit interactive login or replay of uncertain tool calls.
6. [ ] Implement `auth status` without displaying tokens and `auth logout` removing locally saved credentials for the bound identity. Invalidate in-memory credentials across relevant processes. Define handling when multiple configured names refer to the same identity.
7. [ ] Test secret-store failures, denied consent, callback replay/mismatch, timeouts, cancellation, refresh races, changed project endpoints and malicious metadata/redirects. Use local fixtures and a fake secure-store adapter for default tests.

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

- Completed tasks: None.
- In-progress task: None.
- Changed paths / commits: None.
- Tests run and results: None; planning only.
- Decisions approved: None; see overview decision register.
- Blockers / remaining questions: Resolve the contract gates above before affected work.
- Next action: Complete dependencies, inspect their contracts, then begin step 1.
