# Self-uninstall

User requirement: provide an explicit operation that removes ripmcp itself and
cleans its owned resources, not only registered MCP servers.

Candidate syntax: ripmcp --uninstall-everything. Exact spelling is undecided.

## Proposed safety contract

- Preview exact deletion targets and anything that must be preserved.
- Confirm with [y/N]; support -y for unattended use without expanding scope.
- Without a terminal, require -y and fail before changes otherwise.
- Stop owned servers and the supervisor before deleting their resources.
- Remove owned configuration, installation data, state, logs, caches, and runtime
  artifacts, including recorded custom storage locations.
- Remove the ripmcp executable and installation-created symlinks or shell setup
  only when their ownership and installation method are known.
- Never delete PATH directories or broadly rewrite shell configuration.
- For package-manager installations, use the owning manager's supported uninstall
  flow or report the required action rather than deleting its tracked binary.
- Preserve shared runtimes, caches, images, external server data, and resources
  whose ownership cannot be established. Report these exclusions explicitly.
- Preserve project .ripmcp directories in v1 and state this exclusion in the
  cleanup preview. Do not recursively search the filesystem for them.
- Preserve retry metadata on partial failure, report failures, and remove the
  executable last. Do not claim successful complete removal when items remain.

## Open decisions

- Agreed: --include-projects is deferred beyond v1; project configuration remains.
- How will installation record owned paths and distinguish standalone binaries
  from package-manager installations?
- What cleanup can be guaranteed for installations made before ownership tracking?
- Does uninstall-everything imply server-data deletion for exclusively owned data,
  or require an additional explicit choice?

This is a proposed implementation contract. The self-uninstall feature itself is
requested; ownership boundaries and failure behavior still need agreement.
