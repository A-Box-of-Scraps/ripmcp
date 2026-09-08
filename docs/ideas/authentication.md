# Authentication

Agreed: OAuth support is required in v1.

Proposed UX: explicit auth login, auth status, and auth logout commands per server.
Login opens a browser, receives a loopback callback, and stores credentials outside
project configuration. Ordinary noninteractive calls never wait for browser login;
they return an actionable authentication-required error. Refresh credentials when
supported; otherwise request login again. Headless login behavior remains open.

Follow MCP 2026-07-28 Basic / Authorization for HTTP OAuth, including metadata
discovery, client registration, PKCE, response validation, and resource binding.
Client registration strategy and secure credential storage need technical design.
Do not promise compatibility with every provider before defining that profile.
Local stdio credential configuration is separate from the HTTP OAuth flow.

Bind credentials to the actual server resource and authorization issuer, not only
a user-chosen server name. A project endpoint override must not receive another
endpoint's saved credentials.

These implementation and command choices remain proposals.

## Blocking login proposal

User suggests login waits for the user to complete browser authorization.
Recommendation: block by default until callback validation, token exchange, and
credential persistence finish. Return success only then, not when the browser
opens. auth status is optional afterward, not a required completion-polling step.

Provide a bounded, configurable timeout and cancellation, with explicit failures
for denial, timeout, callback validation, token exchange, and storage errors.
Print login instructions immediately to stderr. The agent should tell the user
before calling login, since its harness may buffer command output. Explicit login
may wait; ordinary tool invocation must not start an interactive login implicitly.
Headless behavior remains a separate technical-design question.
