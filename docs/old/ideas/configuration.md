# Configuration and storage

Agreed: support user-wide and project-specific configuration.

User suggestion: .ripmcp for projects and a directory under ~/.local for user data.

## Agreed Linux layout

- Project configuration: <project>/.ripmcp/config.json.
- User configuration: $XDG_CONFIG_HOME/ripmcp/config.json.
- Managed installation data: $XDG_DATA_HOME/ripmcp/.
- Logs and persistent operational state: $XDG_STATE_HOME/ripmcp/.
- Disposable caches: $XDG_CACHE_HOME/ripmcp/.
- Supervisor sockets: $XDG_RUNTIME_DIR/ripmcp/.

The XDG defaults are ~/.config, ~/.local/share, ~/.local/state, and ~/.cache
respectively. The data directory is share, not shared. Honor valid XDG overrides.
Runtime-directory fallback behavior remains to be specified.

Reference: Freedesktop XDG Base Directory Specification, version 0.8, sections 2-3.

Keep generated data and credentials out of project configuration. Create storage
directories only when needed. Other operating-system layouts remain undecided.

## Open semantics

- Project discovery from subdirectories and its search boundary.
- Agreed: project settings override global settings; exact merge rules remain open.
- Agreed: an explicit trust system for the current project's .ripmcp configuration.
  Agreed: keep trust approvals outside the repository, bind them to canonical
  project location and configuration content, and require reapproval on changes.
  Untrusted overrides must not execute commands or redirect authenticated requests.
- Explicit user/project flags for configuration-changing commands and their default.
- JSON is a proposed native format, not yet an agreed decision.
