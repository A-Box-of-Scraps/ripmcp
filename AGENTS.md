# Repository Guidelines

ripmcp is a CLI for managing and invoking MCP servers, minus the bloated ceremony.
It brings the power of MCP to AI agent harnesses built on the idea that Bash Is All You Need.

## Project Structure

Here is an overview of the project:

```
.cargo/config.toml
.github/
docs/
lints/
site/
src/
tests/
.gitignore
.gitattributes
AGENTS.md
Cargo.toml
CHANGELOG.md
clippy.toml
LICENSE
README.md
```

## Development

- Do not make changes to README.md unless it is explicitly requested by the user.
- Do not add comments unless they explain unexpected or complex behavior, or when documentation is explicitly requested by the user. In all cases, keep them concise.
- Use explicit types for non-primitive `let` bindings.

## Validation

Validate changes with:

```sh
cargo fmt -q && dprint fmt --log-level=silent # Format changes direcly instead of checking first and then fixing formatting issues.
cargo clippy -q --all-targets -- -D warnings
cargo dylint --all -- --locked --all-targets
(cd lints/explicit-local-types && cargo fmt -q && cargo test -q --locked)
jscpd .
cargo test -q
```

## Commits & Pull Requests

- Follow the Conventional Commits specification for commit messages.
- Pull request summaries should include the related issue(s), a brief description of the changes, and how the changes were tested.
