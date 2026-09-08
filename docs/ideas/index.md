# ripmcp ideas

Status: discussion notes, not an approved implementation plan.

Goal: a small, shell-friendly MCP CLI for agents and people.

Notes:

- `v1-scope.md`: requested features, proposed boundaries, and additions.
- `server-management.md`: installation, enablement, and process ownership.
- `tool-workflow.md`: discovery, invocation, and large-result inspection.
- `configuration.md`: configuration scope and proposed storage locations.
- `self-uninstall.md`: explicit removal of ripmcp and its owned resources.
- `authentication.md`: OAuth requirement and proposed login workflow.

## Open decisions

1. Agree on ownership boundaries and failure handling for clean uninstall.
2. What configuration formats should installation accept?
3. Should result inspection use external jsonshape and jq, built-in commands, or both?
4. Confirm configuration locations and project discovery/merge rules.
5. Which remote authentication flows and older protocol versions must v1 support?

No command syntax or architecture is final. Keep user requirements separate
from proposals as discussion continues.

## Clarified user direction

- Installation covers remote JSON configuration and local servers launched through
  npx, uvx, or Docker, not only registration of preinstalled commands.
- Local servers should remain available across tool calls until stopped.
- Installation prepares and validates by default, with an explicit skip-verification
  flag. Ordinary uninstall preserves caches and server data.
- Requested: optional clean uninstall with confirmation and -y support; the precise
  cleanup safety contract remains a proposal in server-management.md.
- Per-tool enable/disable is required in v1. Keeping only useful tools available
  is part of the minimal-product goal, not an optional extra.
- Tool policy is default allow: existing and newly discovered tools are enabled
  unless explicitly disabled. No approval step for new tools.
- Support both user-wide and project-specific configuration.
- Linux storage layout in configuration.md is agreed.
- Provide explicit self-uninstall with cleanup, including removal of ripmcp itself.
- Self-uninstall preserves project configuration in v1; --include-projects is deferred.
- OAuth is required in v1; login UX and implementation profile remain proposals.
- Project overrides global configuration, with explicit project trust.
- Partial failures must be explicit and easy to diagnose.
- Ordinary tool invocation returns its result; shape inspection is separate.
- Agreed: standalone ripmcp shape reads saved JSON without invoking tools.
- Agreed: trust approvals are outside the repository and bound to project location
  and configuration content; configuration changes require renewed approval.
- Agreed: cleanup deletes exclusively owned, tracked resources and preserves
  external, shared, or uncertain-ownership resources.

## Remaining scope decisions

- OAuth login UX, headless behavior, and supported authentication profile.
- Result format and whether shape inspection is external or built in.
- Project discovery, override rules, configuration-write defaults, and trust.
- Confirm clean-uninstall ownership boundaries and handling of partial failures.

Protocol compatibility, installation rollback, exact command syntax, and supervisor
design also need specification before implementation, but do not require expanding
the agreed feature set.

## Agreed lifecycle behavior

- Tool invocation auto-starts enabled local servers when needed.
- Reuse local servers across calls.
- Keep explicit start and stop commands for local servers.
- Disabled servers never auto-start.
- Remote endpoints have no start/stop operations.
- Automatic idle shutdown is deferred.
- The supervisor architecture remains a proposal; these decisions define behavior.
