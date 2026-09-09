# 06: Local installation and remote registration transactions - 2026-09-08

Status: **Done**

Dependencies: 02, 03, 04, 05.

Contract gates: D03-D05; version resolution and verification process retention.

Read [the overview](index.md) first. Paths below are suggested module boundaries;
inspect existing work before creating or modifying them.

## Intended files

```diff
+ src/install/ (new or modify when introduced by an earlier phase)
+ src/ownership/ (new or modify when introduced by an earlier phase)
+ src/config/ (new or modify when introduced by an earlier phase)
+ tests/install.rs (new or modify when introduced by an earlier phase)
+ tests/support/ (new or modify when introduced by an earlier phase)
```

## Implementation steps

1. [x] Implement the approved named install forms and native JSON import with strict validation. Convert all forms into one typed installation request; parse runtime arguments as argument vectors, not concatenated shell commands. Require trust before processing executable project definitions.
2. [x] Implement npx, uvx and Docker runtime adapters. Check prerequisites, prepare the package/image and capture the exact launch configuration plus resolved version/identity where supported. Respect the approved pinning policy and fail explicitly when a requested version cannot be honored.
3. [x] Allocate dedicated managed locations lazily and journal exclusively owned resources as they are created. Treat shared package caches/images, existing volumes, user paths and bind mounts as preserved resources regardless of server usage.
4. [x] Implement install as a locked staged transaction: validate input and target, prepare resources, initialize/connect, discover tools, then atomically commit the definition and usable state. Do not invoke a tool for verification. Preserve an existing definition on all failed attempts.
5. [x] Implement `--skip-verify` so preparation still happens and the committed server is explicitly unverified. Remote registration skips connection/discovery only; it does not bypass config, scope, trust or ownership validation.
6. [x] Integrate explicit authentication-required failures into remote verification. Ensure the error gives a usable login/retry path even before a server definition is committed; decide whether to retain a non-active pending registration or direct the user through skip-verify registration, login and later discovery.
7. [x] On failure/cancellation roll back owned provisional processes/resources when safe, report residual resources, and retain recovery records for incomplete rollback. Ordinary rollback must not delete shared caches or overwrite concurrent config changes.
8. [x] Return installation results identifying scope, runtime/endpoint, verification state and preserved/prepared resources without secrets. Apply the approved policy for retaining or stopping the process used for successful verification.

## Acceptance criteria

- Fake-runtime tests cover successful npx/uvx/Docker preparation, missing runtimes, bad versions, runtime failure and cancellation.
- Verification connects and discovers but makes zero tool calls; skip-verify still prepares dependencies.
- Remote registration/authentication failures provide a workable recovery path.
- Failed installs do not replace existing definitions or leave falsely successful registration state.
- Interrupted commits and rollback failures retain sufficient ownership/retry records.
- Repository validation gate passes; real-runtime smoke tests remain opt-in.

## References

server-management.md installation; configuration.md; authentication.md; user install commands.

Idea filenames refer to `../ideas/`. The user's latest command list takes
precedence over tentative spellings in those notes.

## Handoff (update whenever work stops)

- Completed tasks: Steps 1-8 and all default acceptance tests.
- In-progress task: None.
- Changed paths / commits: New `src/install/`,
  `src/supervisor/manager/install.rs`, `tests/install.rs`, and `tests/install/`;
  integration changes in `src/lib.rs`, configuration staging, ownership models,
  storage transactions, remote authentication options, supervisor launch/guard/IPC,
  and their existing CLI/lifecycle/supervisor fixtures. Implementation trackers and
  contracts updated. No commits made; no README or product documentation changes.
- Tests run and results:
  - `cargo fmt -q`: passed.
  - `cargo clippy -q --all-targets -- -D warnings`: passed.
  - `cargo test -q`: passed, including 25 installation tests and one intentionally
    ignored real-runtime smoke test.
  - `cargo dylint --all -- --locked --all-targets`: passed.
  - `(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)`:
    passed, including both UI fixtures.
  - `git diff --check`: passed.
  - During earlier full-suite runs, the existing OAuth callback-rebind test once
    failed with `AddrInUse`, and the existing supervisor queue test once failed to
    observe saturation. Both passed in isolation and subsequent full-suite runs
    passed. Neither assertion was weakened. The alternate-runtime supervisor
    fixture now explicitly creates a private runtime directory instead of
    accidentally testing the shared fallback.
- Decisions approved: D03-D05 and the existing pinning/retention contracts used
  without changing the approved command surface. Engineering details below.
- Blockers / remaining questions: None for phase 06. Real registry/daemon smoke
  testing was not run; it remains explicit and opt-in. Recovery execution belongs
  to phase 08; phase 06 retains the necessary records rather than guessing cleanup.
- Next action: Read phase 07 and the integration contracts; implement cross-server
  discovery, tool policy/workflow, and shape without weakening installation trust.

### Implemented transaction and recovery boundary

Installation requests are validated on both sides of authenticated supervisor IPC
(version 3). The daemon coordinates preparation and verification under its
maintenance barrier, the target configuration lock, and the scoped process slot.
Definitions remain private in a staged effective view until commit. File bytes,
parent-directory identity, selected project, and imported-project approvals are
rechecked. Each runtime command checks trust immediately before spawning; queue,
lock, preparation and discovery waits consume one operation budget.

Ownership records contain the original server-definition snapshot (references,
not resolved secrets), requested/resolved origin, runtime executable, intended
configuration digest, verification state, and install operation progress. Install
operations identify both preparation and scoped process leases. The registered
intent is durable before the configuration rename, which is the publication point;
operation finalization follows. Late I/O failure can leave an uncertain commit,
not permission to overwrite configuration. Recovery must inspect the saved digest,
definition, registration and lease/container records together.

Preparation and verification children use the existing pidfd/process-group guard.
Preparation checks the child's actual exit status and bounds captured output.
Rollback stops only newly owned processes, retains failed container/lease records,
and reports the installation ID for recovery. A separate bounded cleanup budget
allows teardown after cancellation or expiry. Shared runtime storage and user paths
are never deleted. No dedicated installation data directory is necessary for these
adapters, so it is not allocated speculatively.

Successful local verification performs discovery only and retains the supervised
process. Skipped verification still prepares dependencies, records `unverified`,
and starts no MCP server. Remote verification uses the noninteractive auth provider
against the staged, rechecked target and closes its client before commit. Failed
remote authentication directs users to configure OAuth or required secret
references, register with `--skip-verify`, then login and discover; project writes
require renewed trust and scope shadowing must be considered. Local secret errors
do not receive misleading OAuth instructions.

Reports use a typed version-1 `installation` object containing scope, runtime or
endpoint identity digest, verification, process retention, prepared/preserved
resource classifications, trusted shadowing and project reapproval. Raw arguments,
endpoint queries, package specs, secret references and resolved credentials are
not returned.

### Runtime resolution profile

- npx: require existing npx, npm and Node executables. Resolve registry package
  names/scopes and selectors with `npm view ... version --json`, require an exact
  version, then populate runtime storage using npx with explicit `--package` and
  an absolute Node executable running an empty script. No MCP tool is called.
- uvx: accept a distribution name, `name==version`, `name@version`, or
  `name@latest`. Prepare with `--from`, refresh metadata, and inspect the installed
  distribution through isolated Python. Record `name==version`; launch with
  `uvx --from name==version name`, preserving server arguments as separate values.
  Python downloads are disabled during preparation and subsequent starts.
- Docker: pull through the existing daemon and inspect repository digests. Record
  and launch an immutable digest; an explicitly requested digest must be present.
  Images remain shared, including newly pulled images.
- Unresolvable identities, unsupported package-source forms, mismatched exact
  versions, absent prerequisites and runtime failures fail explicitly. No runtime,
  Python interpreter, foreign config importer or package manager is installed.

Official runtime references inspected September 8, 2026:

- `https://docs.npmjs.com/cli/v11/commands/npm-view/`
- `https://docs.npmjs.com/cli/v11/commands/npm-exec/`
- `https://docs.astral.sh/uv/guides/tools/`
- `https://docs.astral.sh/uv/reference/cli/`
- `https://docs.docker.com/reference/cli/docker/image/pull/`
- `https://docs.docker.com/reference/cli/docker/image/inspect/`

The ignored `real_runtime_smoke` test requires explicitly selected
`RIPMCP_SMOKE_RUNTIME` and `RIPMCP_SMOKE_PACKAGE`, uses the caller's runtime PATH
with isolated HOME/XDG directories, and must select a package/image compatible
with the approved MCP revision. Default tests use only sandboxed fake executables
and local HTTP fixtures.
