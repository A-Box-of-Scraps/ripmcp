# Validate changes and maintain documentation

[Documentation](../README.md) / How-to guides

Use the [source map](../reference/development.md) to locate implementation and
test evidence before changing a behavior claim. Run these commands from a source
checkout, not an installed release archive.

## Validate repository changes

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
passes. See [remaining validation limits](../reference/compatibility.md#validation-status).

## Regenerate and test the manual

`docs/reference/manual.md` is the authored source for the standalone `ripmcp(1)` reference.
`man/ripmcp.1` at the repository root is generated and committed; do not edit it
directly. Keep command behavior aligned with the implementation and leave parser
help unchanged. Avoid repository-relative links and archive references in the
manual, so it remains useful offline.

Generation requires Python 3.11+ and Pandoc 3.11. The pinned converter and empty
date make output reproducible; the footer version comes from `Cargo.toml`.
After editing the source or changing the package version, run from the root:

```sh
dprint fmt --log-level=silent
python3 scripts/src/build_manual.py
python3 scripts/src/build_manual.py --check
python3 -B -m unittest discover -s scripts/tests -p 'test_*.py'
man -l man/ripmcp.1
```

The manual tests also require groff and man-db. They check rendering and isolated
manual lookup; release tests check archive contents and checksums without building
Rust or contacting servers. Rust tests check public command coverage, native JSON
examples, and the offline tutorial. Documentation link tests exclude `docs/old/` and check local targets
and heading anchors. CI rejects stale generation. Ordinary Cargo builds and release
packaging use the committed page and need no document converter.

## Review documentation changes

- Keep tutorials guided and reproducible, with prerequisites and checkpoints.
- Keep task steps in how-to guides, exact fields/options in reference, and design
  rationale in explanation. Link to detail instead of duplicating it everywhere.
- Check CLI snippets against the parser and JSON examples against the native
  schema. Mark provider/package placeholders rather than imply compatibility.
- Preserve the call-once workflow; alternative call forms are not steps to execute
  repeatedly against a side-effecting tool.
- Keep relative links valid after moves. Do not rewrite historical claims as
  current evidence, or mark readiness gaps complete through documentation edits.
