# Source and validation map

[Documentation](../README.md) / Reference

Use executable behavior to maintain these docs. The archive records history, not
an independent current specification. In particular, later generic authentication
support supersedes earlier public-client-only notes, and cleanup reports use the
actual `completed`/`failures`/`plan` model rather than early proposed report fields.

## Where to check behavior

| Topic                               | Implementation                                                                                                                           | Tests                                                                                                                                   |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Public grammar, dispatch, exits     | [cli.rs](../../src/cli.rs), [lib.rs](../../src/lib.rs), [error.rs](../../src/error.rs)                                                   | [cli.rs](../../tests/cli.rs), [coverage.rs](../../tests/validation/coverage.rs)                                                         |
| Configuration, scope, trust         | [schema.rs](../../src/config/schema.rs), [effective.rs](../../src/config/effective.rs), [trust](../../src/trust/mod.rs)                  | [config.rs](../../tests/config.rs)                                                                                                      |
| Paths and private storage           | [paths.rs](../../src/storage/paths.rs), [directory.rs](../../src/storage/directory.rs)                                                   | [storage.rs](../../tests/config/storage.rs)                                                                                             |
| Installation and runtime resolution | [input.rs](../../src/install/input.rs), [resolve.rs](../../src/install/preparation/resolve.rs)                                           | [install.rs](../../tests/install.rs), [local lifecycle](../../tests/validation/local.rs)                                                |
| Local processes and status          | [launch.rs](../../src/supervisor/launch.rs), [status.rs](../../src/supervisor/manager/status.rs)                                         | [lifecycle.rs](../../tests/lifecycle.rs), [supervisor.rs](../../tests/supervisor.rs)                                                    |
| Protocol and HTTP                   | [protocol.rs](../../src/mcp/protocol.rs), [http.rs](../../src/mcp/http.rs)                                                               | [stdio](../../tests/mcp_stdio.rs), [HTTP](../../tests/mcp_http.rs)                                                                      |
| Authentication and interaction      | [auth schema](../../src/config/authentication.rs), [configure](../../src/auth/configure.rs), [interaction](../../src/mcp/interaction.rs) | [auth.rs](../../tests/auth.rs), [auth_configure.rs](../../tests/auth_configure.rs), [interactive](../../tests/lifecycle/interactive.rs) |
| Discovery, policy, call input       | [tools](../../src/tools/mod.rs), [policy](../../src/tools/policy.rs), [call.rs](../../src/call.rs)                                       | [tools.rs](../../tests/tools.rs)                                                                                                        |
| JSON output and inspection          | [output.rs](../../src/output.rs), [shape.rs](../../src/shape.rs)                                                                         | [contracts.rs](../../tests/contracts.rs), [shape.rs](../../tests/shape.rs)                                                              |
| Cleanup and self-removal            | [cleanup](../../src/cleanup/mod.rs), [plan](../../src/cleanup/plan.rs), [self plan](../../src/cleanup/self_plan.rs)                      | [uninstall.rs](../../tests/uninstall.rs), [self_uninstall.rs](../../tests/self_uninstall.rs)                                            |

## Repository validation

From the repository root, follow [AGENTS.md](../../AGENTS.md):

```sh
cargo fmt -q && dprint fmt --log-level=silent
cargo clippy -q --all-targets -- -D warnings
cargo dylint --all -- --locked --all-targets
(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)
jscpd . --no-tips
cargo test -q
```

These commands require the corresponding formatter/linter tools as well as the
configured Rust/linker toolchain. Default tests isolate application HOME/XDG state
and use fixture servers; do not replace them with personal credentials or public
services to make a check pass. Several fixtures require Python 3 at
`/usr/bin/python3`.

The ignored real-runtime smoke in [install tests](../../tests/install/local.rs)
requires deliberate `RIPMCP_SMOKE_RUNTIME` and `RIPMCP_SMOKE_PACKAGE` selection.
The opt-in disk-full test in [failure tests](../../tests/validation/failures.rs)
requires namespace/mount support. Record omitted or failed checks, not inferred
passes. See [remaining validation limits](compatibility.md#validation-status).

## Documentation maintenance

- Keep tutorials guided and reproducible, with prerequisites and checkpoints.
- Keep task steps in how-to guides, exact fields/options in reference, and design
  rationale in explanation. Link to detail instead of duplicating it everywhere.
- Check CLI snippets against the parser and JSON examples against the native
  schema. Mark provider/package placeholders rather than imply compatibility.
- Preserve the call-once workflow; alternative call forms are not steps to execute
  repeatedly against a side-effecting tool.
- Keep relative links valid after moves. Do not rewrite historical claims as
  current evidence, or mark readiness gaps complete through documentation edits.

Historical entry points: [ideas](../old/ideas/index.md),
[implementation tracker](../old/implementation/index.md), and
[generic authentication](../old/implementation/10-generic-authentication.md).
