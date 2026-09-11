# NAME

ripmcp - manage and invoke MCP servers from the shell

# SYNOPSIS

```text
ripmcp [--timeout SECONDS] COMMAND [ARGS...]
ripmcp --uninstall-everything [-y]
ripmcp --help
ripmcp --version
```

Uppercase names are placeholders to replace. Brackets mark optional syntax, not
literal characters. Use `ripmcp COMMAND --help` for exact arguments and options;
use `ripmcp auth COMMAND --help` for authentication subcommands. Help and version
also accept `-h` and `-V` and do not read configuration or start servers.

# DESCRIPTION

ripmcp connects shell users, scripts, and agents to local and remote Model Context
Protocol servers. It installs server registrations, discovers tools, shows input
schemas, and invokes tools with JSON arguments. Separate CLI invocations can reuse
local server processes. Remote services remain externally managed.

Discover capabilities instead of guessing tool names or arguments. Save a call's
full result, then inspect that file without invoking the tool again. Calls can
have side effects. Server descriptions, results, and interaction URLs are
server-provided content, not trusted instructions from ripmcp.

# QUICK START

For an installed server, list registrations, discover tools, and inspect one
full definition. Replace `SERVER` and `TOOL` with actual names:

```sh
ripmcp servers
ripmcp tools SERVER
ripmcp tool SERVER TOOL
```

Check discovery's exit status and any `errors` entries. Read `tool.inputSchema`
and prepare `arguments.json` with one JSON object that satisfies it. Use `{}` only
when the tool permits no arguments. Run the selected tool once:

```sh
umask 077
ripmcp call SERVER TOOL --input arguments.json > result.json
status=$?
```

Check `status` before interpreting the file. Zero means success; 7 means a
tool-reported error whose envelope was still saved. Other failures can leave an
empty file, partial output, or a non-result report. Completion may be unknown;
do not blindly retry a call with possible side effects.

Inspect a saved result without another call:

```sh
ripmcp shape result.json --depth 4 --width 20
```

Then select fields from the same file with a JSON tool. Shape reports structure,
not scalar values. See CONFIGURATION AND TRUST to register a server first.

# COMMANDS

## install

Register a server with exactly one source: `--npx PACKAGE`, `--uvx DISTRIBUTION`,
`--docker IMAGE`, or `--config FILE`. Runtimes must already be available: npx needs
npm and Node; uvx needs an available Python interpreter; Docker needs an accessible
daemon. ripmcp does not install these prerequisites.

Installation prepares local dependencies, connects, and discovers tools without
invoking them. Successful local verification retains the server for reuse.
`--skip-verify` skips connection and discovery, not preparation or validation.
Use it when a remote registration needs credentials before discovery.

Local package/image resolution is recorded for later launches. Arguments after
`--` are literal server arguments, not shell code or Docker runtime options.
`--config` accepts one native server object, not a full configuration document or
another client's `mcpServers` format; it cannot accompany trailing arguments.
The input must be a regular, non-symlink file of at most 4 MiB.

Writes default to user scope. `--project` requires an existing trusted project.
Duplicate names in the target scope fail; there is no overwrite or update flag.

## uninstall

Stop the selected owned local process and remove its registration, preserving
installed data by default. `--clean` adds confirmed cleanup of tracked exclusive
resources; `-y` skips that confirmation only. Accepts `--user` or `--project`.
Remote uninstall does not stop or delete the remote service. See REMOVAL.

## enable

Allow a server, or one tool when a tool name is supplied. Enabling does not start
the server. Accepts `--user` or `--project`; omission targets user configuration.
Tool policy changes can require live discovery to validate the name.

## disable

Block future use of a server or tool. Disabled tools are hidden from normal
discovery and blocked on direct calls. Disabling does not stop a running process
or retract a request already sent. New tools are allowed unless disabled by name.
Scope selection is the same as for enable.

## servers

List effective registrations, provenance, enablement, trust requirements, process
state, and saved health. This is passive: it neither starts servers nor checks
endpoints. Enablement, process state, and health are separate facts. Local health
observations become stale after 60 seconds or a configuration revision change.
Remote process state is `not_managed` and health is `unknown`.

## start

Start and retain an enabled installed local server. Discovery and calls can also
start it automatically. Remote start is unsupported.

## stop

Stop an owned local server. An already-owned instance can be stopped even when
disabled or its project is now untrusted, without executing the edited definition.
Remote stop is unsupported.

## tools

Discover tools across enabled servers, or on one named server. Discovery is fresh
and can start local servers. `tools SERVER --all` includes disabled tools, not
disabled servers; `--all` requires a server name. Reports contain concise tool
metadata and an `errors` array. Incomplete discovery emits valid entries and safe
failure identities, then exits 8. Missing results are not proof that an unavailable
server has no tools.

## tool

Return the full definition for `SERVER TOOL`, including its input schema, under
the report's `tool` field. Performs discovery and does not bypass disabled policy.
General JSON Schema validation of call arguments remains server-side.

## call

Invoke a tool with exactly one JSON argument object. These are alternative forms,
not a sequence of calls:

```text
ripmcp call SERVER TOOL JSON
ripmcp call SERVER TOOL --input FILE
ripmcp call SERVER TOOL --input -
ripmcp call TOOL JSON
ripmcp call TOOL --input FILE
ripmcp call TOOL --input -
```

Inline JSON should be shell-quoted. Prefer a file or stdin for sensitive input to
avoid shell history and process-argument exposure. With `--input`, `-` means stdin;
do not also supply positional JSON. Input must be an object even for a tool with
no arguments.

Omitting the server triggers discovery across eligible servers. Exactly one match
and complete discovery are required. Ambiguity emits candidates and exits 2;
incomplete discovery emits a report and exits 8. Neither invokes a tool. Use a
qualified call to avoid unrelated servers preventing shorthand resolution.

`--interactive` permits structured URL input requests and waits for the final
result under the original deadline. Follow the attributed instructions on stderr.
This does not parse arbitrary login text, support form input, or retry uncertain
delivery. Without the flag, input-required results do not continue.

## shape

Inspect saved JSON offline with `shape FILE` or `shape -` for stdin. Reads no
configuration or credentials and contacts no server. Any JSON root is accepted.

`--depth` defaults to 8 and accepts 1-64. `--width` defaults to 100 and accepts
1-10000. Root depth is zero; width applies separately to each container. A global
budget permits at most 100000 inspected nodes.

The report contains `shape` and `limits`. Nodes have a `type`; objects add `fields`,
and arrays add exact `length` and ordered per-element `items`. Container
`truncated` and `omitted` fields describe excluded children. Bounds limit the
report, not the saved file; a bounded report still exits 0. Scalar values are not
shown, but field names can contain secrets: shape is not a general redaction tool.

## trust

Preview and approve the selected project's exact configuration bytes at its
canonical location. Review the actual file as well as the redacted preview.
Both stdin and stderr must be terminals. There is no path argument, `-y`, or
project-creation behavior. Declining approval exits 3. Trust allows configured
operations; it is not a sandbox for local server code. See CONFIGURATION AND TRUST.

## auth

Configure remote credentials, log in, inspect local credential status, or log out.
Local servers instead use explicit environment references. See AUTHENTICATION.

## auth configure

Configure one remote HTTPS registration offline. Choose bearer prompt, bearer
environment reference, custom header, or OAuth client ID. Only configure accepts
`--user` and `--project`. Success records configuration, not provider acceptance.
Environment-reference values need not exist until later use. Changing mechanisms
does not revoke old tokens or necessarily delete shared credentials.

## auth login

Run explicit browser OAuth login for the effective remote server and wait for
secure credential storage. Prints instructions on stderr and opens a browser
when possible; otherwise open the printed URL manually on the same machine while
the command remains running. Ordinary discovery and calls do not initiate login.
Not used for bearer tokens or local servers.

## auth status

Inspect local authentication state without checking provider acceptance. OAuth
`saved` means credentials exist locally. Bearer `available` means local resolution
and syntax checks passed; missing credentials are errors. Bearer status requires an
enabled server; OAuth status can inspect a disabled registration. Both require
project trust when applicable. Custom headers are not managed by this command.

## auth logout

Invalidate local OAuth credentials for the endpoint/registration partition, also
affecting aliases sharing that partition. For managed bearer storage, delete the
token but leave its reference for reconfiguration. Environment and external
keyring values remain externally managed: logout exits 10 without a JSON success
report. Bearer logout requires enablement; OAuth logout does not. Both require
project trust when applicable. Does not revoke provider-side tokens
or undo requests already authorized. Custom header collections are not managed.

# INPUT AND OUTPUT

Data responses are one complete JSON value plus a newline on stdout. Diagnostics,
previews, prompts, and login instructions go to stderr. Help and version are text
exceptions. Do not merge stderr into a saved JSON result.

Call output is the full MCP tool-result envelope, not the JSON-RPC wrapper or only
its text content. Content, structured content, extensions, and exact JSON numbers
are preserved as values; whitespace and key order are not guaranteed. A tool's
`isError: true` envelope is written before exit 7. A stdout write failure takes
precedence and exits 1, potentially leaving partial output. Non-call reports use
`schema_version: 1`, but their fields differ by command.

There is no result cache, automatic save, silent truncation, or automatic replay
after uncertain delivery. Empty output, cancellation, timeout, or a failed pipe
does not prove the server performed no action. Check application state before
deciding whether a repeat is safe.

## Deadlines

`--timeout SECONDS` is a positive integer accepted before or after the command.
The default is 60 seconds, or 300 for OAuth login and credential configuration.
Trusted configuration can replace these defaults; the CLI override wins. Shape
ignores configuration and has its own 60-second default.

One operation budget includes preparation, waits, discovery, invocation, and
interaction. Progress does not reset it. Destructive confirmation precedes the
mutation deadline. Ctrl-C exits 130; timeout exits 9. Neither promises rollback.
Cancelling a reused local call does not kill the shared server. Process cleanup
can require an additional bounded grace period.

Trust is an exception: its synchronous terminal prompt is not timed. After
approval, the timeout applies separately to maintenance-lock acquisition and the
approval write, rather than one end-to-end budget.

## Limits

Configuration and native install input are limited to 4 MiB, call file/stdin input
to 16 MiB, and shape input to 64 MiB. Strict JSON parsing rejects duplicate object
keys and enforces nesting bound 128. Inline input also faces shell/OS limits.
Default protocol bounds are 16 MiB per frame or SSE event and cumulative discovery
of 32 MiB, 10000 tools, or 1000 pages. Oversized transport input fails rather than
returning shortened results.

# CONFIGURATION AND TRUST

## Register a remote server

Save this single native server object as `remote.json`, replacing the example
endpoint with your compatible server's HTTPS URL:

```json
{
  "definition": {
    "kind": "remote",
    "url": "https://mcp.example.com/mcp",
    "transport": "streamable_http",
    "authentication": "none"
  }
}
```

```sh
ripmcp install remote --config remote.json --skip-verify
```

Configure credentials if required, then verify with `ripmcp tools remote`.
For automatic OAuth registration, use `"authentication": "oauth"` in the import
and run `ripmcp auth login remote` after installation. OAuth endpoint URLs must
be query-free. Operational endpoints require HTTPS except unauthenticated loopback
HTTP; credential headers always require HTTPS.

## Install a local definition

Choose one local source, for example `ripmcp install local --npx PACKAGE`.
For explicit environment references or a working directory, import a native object:

```json
{
  "definition": {
    "kind": "local",
    "runtime": "npx",
    "package": "@example/mcp-server@1.2.3",
    "transport": "stdio",
    "args": [],
    "env": { "API_TOKEN": { "env": "SERVICE_TOKEN" } },
    "cwd": "/absolute/server/workspace"
  }
}
```

Replace the package, variable mapping, and path before installation. This is not
a tested public server recommendation. Other runtimes are `uvx` and `docker`.
Adding local JSON directly to configuration does not prepare an installation;
use `install`. Runtime or package edits can invalidate the recorded preparation.

Server objects default to `enabled: true` and `disabled_tools: []`. Secret fields
take references, not raw strings: `{"env":"VARIABLE"}`, `{"keyring":"ID"}`, or
ripmcp-generated `{"stored":"ID"}`. Never invent managed stored IDs. Environment
values are resolved from the invoking process when needed, not captured at daemon
startup. External keyring references use Secret Service attributes
`application=ripmcp`, `namespace=references-v1`, and `identity=ID`.

## Project setup and selection

User and project configuration use the same document format. To create a project,
write `.ripmcp/config.json` at the intended root only if it does not already exist:

```json
{
  "schema_version": 1,
  "servers": {},
  "timeouts": {
    "operation_seconds": 60,
    "login_seconds": 300
  }
}
```

The `timeouts` object is optional; supplied values must be positive integers.
Unknown fields, duplicate keys, malformed selected files, and unsupported schema
versions fail. A `.ripmcp` directory without `config.json` does not select a project.

Review the file and run `ripmcp trust` in a terminal. Then use `--project` for
project mutations and renew trust after changed bytes, including CLI changes.
Unchanged semantic no-ops preserve trust. Moving a project also requires approval.

Selection searches from canonical cwd to the filesystem root for the nearest
ancestor `.ripmcp/config.json`; nested projects are not combined. A project server
replaces the entire same-named user object. An untrusted override blocks that
name rather than falling back to the user definition. An approved project timeout
section replaces the entire user section; omitted members use built-in defaults.

Scope flags choose write targets, not read overrides. Calls, discovery, lifecycle
commands, and auth login/status/logout use effective selection. To access a
shadowed user server, run outside the overriding project. Inspect `servers`
provenance and `trust_required` when selection is unexpected.

# AUTHENTICATION

For a remote server accepting bearer tokens, choose a hidden prompt with secure
storage or an environment reference for automation:

```sh
ripmcp auth configure remote --bearer
ripmcp auth configure remote --bearer-env SERVICE_TOKEN
```

These are alternatives. Supply the raw token without the `Bearer ` prefix.
Environment setup saves only the variable name; supply its value to each later
invocation through your secret manager. Check `auth status`, then `tools remote`
for a real discovery check.

For an API key header, use `auth configure remote --header X-API-Key` to prompt,
or append `--header-env SERVICE_API_KEY`. The helper rejects Authorization and
transport-owned headers; use managed bearer setup for a bearer token. Check custom
headers through discovery, not auth status.

Changing authentication retains unrelated custom headers. Bearer/OAuth setup
removes an explicit Authorization header; custom-header setup replaces only its
named header and retains any explicit Authorization header. Remove obsolete
entries from the selected configuration yourself and renew project trust if needed.
There is no header-removal option.

For OAuth with your own registered public client:

```sh
ripmcp auth configure remote \
  --oauth-client-id YOUR_CLIENT_ID \
  --issuer https://identity.example.com \
  --scope tools:read
ripmcp auth login remote
```

Replace the issuer, client ID, and scope. The issuer must exactly match selected
metadata. Repeat `--scope` as needed. Register this exact callback:

```text
http://127.0.0.1:42813/oauth/callback
```

For a client requiring a secret, add `--client-secret` to prompt or
`--client-secret-env VARIABLE` for a reference. Choose the provider's
`--token-endpoint-auth-method`: `none`, `client_secret_post`, or
`client_secret_basic`. None requires no secret; Post/Basic require one. The helper
defaults to Post when a secret is supplied, otherwise none.

OAuth and prompted secrets require a session bus and an existing unlocked default
Secret Service collection. ripmcp does not unlock it or fall back to plaintext
storage. Environment-only bearer/header setup needs no keyring. Prompts use
`/dev/tty`; tokens over 4094 bytes should use environment references instead.

Local server credentials belong in explicit `definition.env` references, not
auth login/configure. Tool-directed URL interaction uses `call --interactive`,
which is separate from ripmcp's OAuth login.

# SERVER LIFECYCLE

A per-user supervisor owns local connections through private Unix-socket IPC.
Processes can survive CLI exit, and there is no idle shutdown. Reuse depends on
scope, installation identity, configuration revision, resolved runtime/package,
and effective environment, not just a server nickname.

Discovery and calls can start enabled local servers. To both block future use and
stop a local instance, disable it and then stop it. A running process does not
imply enabled policy or fresh healthy status. Use `tools SERVER` for an active
connection check; it does not invoke tools.

# REMOVAL

Ordinary `uninstall SERVER` preserves data. `uninstall SERVER --clean` previews
tracked exclusive deletion targets on stderr and asks for confirmation. Decline
to preview without applying; there is no separate dry-run flag. Nonterminal
cleanup needs `-y`, which skips the prompt, not ownership checks.

Shared caches/images, existing volumes, bind mounts, external resources, and
uncertain ownership are preserved. Uninstall is not provider-side token revocation.
Sign out before unregistering if needed, and revoke tokens with the provider.

On incomplete cleanup, inspect `completed`, `failures`, and `plan`. Server cleanup
reports `plan.preserved`, the retry argument vector `plan.retry`, and
`plan.retry_cwd`. Correct the cause and retry in the recorded directory, preserving
recovery records. Do not treat an argument vector as an unquoted shell string.
Incomplete requested cleanup exits 8.

`--uninstall-everything [-y]` stops owned processes and removes tracked exclusive
user resources and managed credentials. It preserves project `.ripmcp` directories,
shared resources, and untracked contents; it does not search for projects. Retained
project definitions may no longer run after installation records are removed.

Executable removal is last and requires matching recorded standalone installer
provenance. Cargo-built, manually copied, or package-managed binaries are preserved
for manual or package-manager removal, and incomplete removal is reported.
Manually installed manual pages are not automatically tracked. Remove the specific
installed page yourself, or use its package manager; do not remove whole manual
or PATH directories.

# ENVIRONMENT

`HOME` and `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`,
and `XDG_RUNTIME_DIR` select storage locations described under FILES. Overrides
must be absolute without parent-directory components. Invalid config/data/state/
cache overrides use HOME defaults. The runtime base also requires private ownership
and permissions; its fallback ignores `TMPDIR`.

Local children receive only `PATH`, `HOME`, `LANG`, `LC_ALL`, the five XDG variables,
and explicitly mapped `definition.env` references. Other shell variables are not
inherited automatically. For Docker, explicit names are forwarded into the
container. Default cwd is `/` for user definitions and the canonical project root
for project definitions. Configured Docker cwd is the host launch directory,
not a container workdir option.

# FILES

- `.ripmcp/config.json`: nearest ancestor project configuration.
- `$XDG_CONFIG_HOME/ripmcp/config.json`: user configuration; default
  `~/.config/ripmcp/config.json`.
- `$XDG_DATA_HOME/ripmcp/`: managed data; default `~/.local/share/ripmcp/`.
- `$XDG_STATE_HOME/ripmcp/`: trust, ownership, and recovery state; default
  `~/.local/state/ripmcp/`.
- `$XDG_CACHE_HOME/ripmcp/`: cache; default `~/.cache/ripmcp/`.
- `$XDG_RUNTIME_DIR/ripmcp/`: private runtime area; validated `/tmp/ripmcp-UID/`
  fallback. OAuth coordination uses that private fallback independently of XDG.

Storage is created when needed. Sensitive state directories/files require the
effective user and modes 0700/0600. Unsafe existing state is rejected, not silently
repaired. Trust approvals stay outside projects in state `trust.json`. Generated
ownership and retry records are not configuration to copy between projects.
Credential values live in Secret Service or referenced environments, not a
plaintext credential file.

# EXIT STATUS

- **0**: Success, including help/version and bounded shape reports.
- **1**: Internal or I/O failure, including stdout write failure.
- **2**: Invalid CLI/tool input or missing/ambiguous tool name.
- **3**: Configuration, policy, trust, or confirmation requirement failure.
- **4**: Connection or transport failure; call completion may be unknown.
- **5**: Protocol error, violation, or version mismatch.
- **6**: Authentication required or failed.
- **7**: Tool-reported failure; inspect the emitted envelope.
- **8**: Partial discovery or incomplete requested cleanup.
- **9**: Deadline exceeded; call completion may be unknown.
- **10**: Unsupported platform or capability.
- **130**: Cancelled, including declined cleanup confirmation.

# LIMITATIONS

Linux only; managed processes require kernel pidfd support and private Unix
sockets. This implementation targets exact MCP `2026-07-28`, with `server/discover`
and paginated `tools/list`, not legacy initialization or implicit revision fallback.
Local transport is stdio; remote transport is HTTP/1.1 Streamable HTTP POST with
JSON or request-scoped SSE replies. An arbitrary MCP label does not prove compatibility.

No resource/prompt workflows, sampling, roots, forms, subscriptions, task workflows,
registry browsing, automatic package updates, or general result querying. Remote
transport does not use persistent sessions, stream resumption, redirects,
automatic proxy discovery, or automatic retries. Raw local server stderr is
drained but not retained as readable logs.

OAuth requires public HTTPS metadata/provider destinations and S256 PKCE. No
private-network issuers, device/client-credentials grants, alternate callback
ports, or remote-headless redirects. If login stalls, check the session bus,
unlocked keyring, callback port 42813, and exact client registration rather than
disabling transport security.

# SEE ALSO

man(1), manpath(1).

Use `ripmcp --help` and command-specific `--help` for parser-generated syntax.
The repository's `docs/` directory contains longer tutorials, configuration
reference, troubleshooting, and compatibility/validation notes. This manual
describes behavior, not certification of a public provider or package.

For noninteractive access with man-db, use `MANPAGER=cat man 1 ripmcp`.
