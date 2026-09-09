# Why lifecycle, trust, and ownership are separate

[Documentation](../README.md) / Explanation

ripmcp keeps configuration, running processes, authentication, and deletion
authority separate. This explains why an enabled server might be stopped, a running
server might need renewed trust, and uninstall might intentionally preserve files.

## A short-lived CLI can use a long-lived server

Local MCP uses the child's standard streams. Keeping only a PID would not let a
new CLI invocation recover those streams. A per-user background supervisor owns
local connections; CLI processes send requests over an authenticated private Unix
socket. Local processes can therefore survive between calls without a shell agent
keeping their pipes open.

Instances are separated by scope and installation identity. Reuse also depends on
the configuration revision, resolved package/image, runtime executable, and
effective environment. The same nickname does not permit reuse of a process
launched from another project's definition or credentials.

Enabled local discovery/calls can start a server. There is no idle shutdown.
Disabling changes permission for future use; stopping changes process lifetime.
This is why `disable` is not a synonym for `stop`, and why passive `servers` does
not promise fresh health. Remote clients are owned by individual operations;
ripmcp never owns or starts the remote service itself.

## Trust is not a sandbox

A project configuration can select executable packages, pass environment values,
or redirect authenticated requests. Reading a repository must not implicitly grant
those capabilities. Trust therefore approves exact configuration bytes at a
canonical project location, with approval stored outside the repository.

A project definition replaces a same-named user definition as a whole. Field-level
merging could accidentally combine one endpoint with another endpoint's credentials.
An untrusted override blocks the name instead of falling back to a surprising
global target. The CLI and supervisor recheck trust and policy before use, including
after waits and before dispatch.

Approval allows the configured operation; it does not isolate arbitrary server
code or make its instructions trustworthy. Local code still runs with the user's
permissions. Tool descriptions, returned text, and interactive URLs remain content
from that server.

## Credentials belong to destinations, not nicknames

OAuth credentials bind the endpoint, resource, issuer, client identity, and token
endpoint. Explicit registration settings partition caches further. Renaming an
endpoint does not create new authority, and overriding a nickname does not grant
its previous credentials to the new destination.

Ordinary discovery and calls never start browser login. Explicit login may wait
for the user; credential refresh remains bounded and noninteractive. Secret
Service failures do not silently fall back to plaintext files. Reference-based
configuration keeps token values outside the repository, but environment access
and approval still need care.

Logout invalidates future local credential use, not provider-side tokens or calls
already authorized. Likewise, disabling a tool cannot retract a request already
sent to a server.

## One invocation, many views

A tool can mutate state. Retrying after a connection failure risks duplicating an
operation whose first result was lost. ripmcp therefore does not automatically
replay uncertain calls. An interactive continuation is different: it follows the
server's structured input-required state under the same operation budget, rather
than repeating a completed result or failed transport request.

Full result output and offline shape inspection follow the same rule. Save the
result once, inspect its structure, and select fields from that file. Changing
the view should not make another call. Shape bounds the view, while transport
limits reject oversized input rather than returning a silently shortened result.

## Ownership controls cleanup

Using a directory, cache, image, or volume does not make it exclusively owned.
Installation records and filesystem identities establish which resources ripmcp
created and can safely remove. Shared, external, substituted, or uncertain
resources are preserved. Process ownership uses guarded lifecycle identities,
not authority inferred from a PID saved on disk.

Ordinary uninstall removes the registration and stops its owned process but keeps
data. Clean uninstall adds deletion of tracked exclusive resources after preview
and confirmation. `-y` removes the prompt, not the ownership checks. Operations
are coordinated with the supervisor so deletion does not race a new auto-start.

Partial failures retain retry records and produce a nonzero exit. Self-removal
preserves project configuration and removes a provably owned standalone binary
last. Unknown or package-manager executable provenance requires a manual step.
Conservative preservation is part of the cleanup contract, not a claim that every
server artifact has been found.

Next: [project setup](../how-to/projects.md), [credential setup](../how-to/authenticate.md),
or [cleanup procedures](../how-to/remove.md).
