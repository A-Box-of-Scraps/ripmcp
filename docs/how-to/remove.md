# Remove servers or ripmcp

[Documentation](../README.md) / How-to guides

## Remove one registration, preserve data

```sh
ripmcp uninstall remote
```

For a local server, this stops only the selected owned process and removes its
registration. It preserves installed data, caches, and ownership records. For a
remote server, it does not stop or delete the remote service.

The write scope defaults to user. Add `--project` for a project registration;
project configuration changes require renewed trust before later project use.
Server uninstall is not provider-side credential revocation. If needed, sign out
while the server is still registered, and revoke credentials with the provider.

## Also clean exclusively owned resources

```sh
ripmcp uninstall local --clean
```

Read the JSON preview on stderr before answering `[y/N]`. It includes deletion
targets and preserved resources with reasons. Confirmation happens before stopping,
unregistering, or deleting. To preview without applying, decline the prompt.
There is no separate dry-run flag.

For unattended execution after reviewing the intended scope:

```sh
ripmcp uninstall local --clean -y
```

Without terminal stdin/stderr, cleanup requires `-y`. It skips confirmation only;
it does not expand ownership or deletion scope. A preview is still emitted.

Shared runtime caches and images, user directories, bind mounts, existing volumes,
and uncertain resources are not owned just because a server uses them. ripmcp does
not promise to erase every trace of arbitrary server software.

## Recover from partial cleanup

Inspect the JSON report's `completed`, `failures`, and `plan`. For server uninstall,
`plan.preserved` explains exclusions; `plan.retry` is the exact retry argument
vector and `plan.retry_cwd` records the working directory.

Correct the reported cause, retain the ownership/retry records, and repeat the
requested cleanup in the recorded directory. Do not delete the journal to hide a
failure, or run an unquoted retry vector as a shell string. A retry can work after
registration has gone when the retained installation is unambiguous. Ambiguous
records are preserved rather than guessed.

Incomplete requested cleanup exits 8. Resources intentionally preserved by the
ownership contract are not permission to remove them manually without checking.

## Remove ripmcp

```sh
ripmcp --uninstall-everything
```

For unattended removal, append `-y`. This operation stops owned processes and the
supervisor, removes tracked exclusive user resources and owned credentials, and
attempts executable removal last. Review its preview carefully.

It always preserves project `.ripmcp` directories, shared resources, untracked
contents, and uncertain ownership. It does not search the filesystem for projects.
Preserved local project definitions may no longer run after their installation
records or managed resources are removed. There is no `--include-projects` option.

The binary is removed only with recorded, matching standalone installer provenance.
A Cargo-built or manually copied executable is not automatically declared owned.
For package-managed or unknown provenance, it is preserved and the report requests
manual or package-manager removal. The command reports incomplete removal rather
than claiming success. Failures retain retry evidence and prevent premature binary
deletion. Do not remove whole `PATH` directories or broadly edit shell files.

The manual installed with the documented `install` command is not tracked for
self-removal. Remove that exact user-local page separately:

```sh
rm -- "$HOME/.local/share/man/man1/ripmcp.1"
```

For a manual you installed system-wide, remove the corresponding
`/usr/local/share/man/man1/ripmcp.1` with the required permissions. For package-managed
files, use the package manager instead. Do not delete the containing manual tree.

Why these limits exist: [ownership and safe cleanup](../explanation/lifecycle-and-safety.md#ownership-controls-cleanup).
