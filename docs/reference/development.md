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

## Validation evidence

The [command and fault coverage](../../tests/validation/coverage.md) records current
test evidence and remaining integration gaps. See
[compatibility and v1 validation status](compatibility.md#validation-status) for
the user-facing limits. Parser coverage alone does not prove a handler succeeds.

Procedures: [validate changes and regenerate the manual](../how-to/develop.md).
