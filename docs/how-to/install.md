# Install ripmcp and servers

[Documentation](../README.md) / How-to guides

## Build ripmcp from this checkout

Use Linux with Rust/Cargo. On Linux GNU targets the repository linker configuration
also requires Clang and LLD. From the repository root:

```sh
cargo build --release --locked
./target/release/ripmcp --version
./target/release/ripmcp --help
```

Use that binary directly, or add this checkout's `target/release` directory to
your shell's `PATH` for the session:

```sh
export PATH="$PWD/target/release:$PATH"
```

The examples below use `ripmcp` on `PATH`. A Cargo build does not establish the
installer provenance required for automatic executable deletion during
[self-removal](remove.md#remove-ripmcp).

## Install a local server

Choose a package or image that supports the
[implemented protocol profile](../reference/compatibility.md). Replace the
uppercase package/image placeholder in **one** of these commands:

```sh
ripmcp install local --npx PACKAGE
ripmcp install local --uvx DISTRIBUTION
ripmcp install local --docker IMAGE
```

Prerequisites are not installed for you:

| Adapter | Required                                | Preparation and launch                                                       |
| ------- | --------------------------------------- | ---------------------------------------------------------------------------- |
| npx     | npx, npm, Node on `PATH`                | Resolves a registry package to an exact version and prepares it with npx     |
| uvx     | uvx and an available Python interpreter | Resolves the distribution to an exact version; Python downloads are disabled |
| Docker  | Docker CLI and accessible daemon        | Pulls and records an immutable repository digest                             |

npx accepts registry package names, scoped names, and version selectors, not
arbitrary Git/file sources. uvx accepts `name`, `name==version`, `name@version`, or
`name@latest`; it launches the command with the distribution's name. Docker
requires a resolvable repository digest.

To pass server arguments, put them after `--`. They are literal arguments, not a
shell script. With Docker these are arguments after the image, not `docker run`
options. For example, if the chosen package documents a `--port` option:

```sh
ripmcp install local --npx PACKAGE -- --port 9000
```

For environment references or a working directory, import a
[native local server object](../reference/configuration.md#local-definition).
There is no arbitrary `--command` source.

## Register a remote server

Write one native server object to `remote.json`:

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

Replace the example URL, then run:

```sh
ripmcp install remote --config remote.json
```

`--config` accepts a single server object, not a full `schema_version`/`servers`
document or another client's `mcpServers` format. The file must be a regular file,
not a symlink, and at most 4 MiB.

For an endpoint requiring login or token setup before discovery:

```sh
ripmcp install remote --config remote.json --skip-verify
```

Then [configure authentication](authenticate.md) and run `ripmcp tools remote`.

## Choose verification and scope

By default, installation prepares dependencies, connects, and discovers tools.
It does not invoke a tool. Successful local verification leaves the owned process
running for later calls.

`--skip-verify` skips connection and discovery, **not dependency preparation** or
configuration validation. It records the installation as unverified and does not
start a local MCP server for verification.

Writes default to user scope. Use `--project` only after
[creating and trusting a project configuration](projects.md). Project writes
change the approved bytes and require renewed trust before use. Target names must
be unique in their scope; there is no overwrite/update flag.

Do not create a local definition solely by editing `config.json`: execution also
requires the prepared installation record made by `install`. Changing the runtime
or package manually can invalidate that record. Save the desired definition,
uninstall the old registration without `--clean` if data must remain, then install
it again. This is not an atomic update.

If installation fails, read the diagnostic before retrying. Failed preparation or
verification does not become a successful active registration; interrupted commits
or cleanup can retain recovery records. See [troubleshooting](troubleshoot.md).
