# Integrate with AI agents

[Documentation](../README.md) / How-to guides

Use the [ripmcp-manual skill](../../skills/ripmcp-manual/SKILL.md) to give an
agent with shell access guidance for managing MCP servers and invoking their
tools through ripmcp. The harness does not need its own MCP client integration.

## Install the prerequisites

[Install ripmcp and its manual](install.md). In the shell environment used by
the agent, verify:

```sh
ripmcp --version
ripmcp --help
man -P cat ripmcp
```

The skill also uses `rg` to search the manual, so make ripgrep available on the
agent's `PATH`. If the manual is not found, check the
[manual search path instructions](install.md#install-the-manual).

## Install the skill

From a source checkout, copy `skills/ripmcp-manual/` into a skill directory that
your harness discovers. Replace `/path/to/your/agent/skills` below with that
directory. Use a user-level directory for availability across projects, or a
project-level directory if your harness supports it and you want project-only
availability.

```sh
skills_dir=/path/to/your/agent/skills
install -Dm644 skills/ripmcp-manual/SKILL.md \
  "$skills_dir/ripmcp-manual/SKILL.md"
```

This installs the skill instructions, not the binary, manual, or any MCP server.
Skill locations and reload behavior depend on the harness. Reload its skills or
start a new session as required, then check that `ripmcp-manual` is available.
When updating, copy the skill from the same checkout/version as the binary and
manual.

## Use the skill

Ask the agent to use ripmcp for an MCP task, for example:

> Use the ripmcp-manual skill to list my configured MCP servers and discover the
> tools available on one of them. Do not invoke any tools yet.

The skill directs the agent to read the manual and command-specific help as
needed rather than embedding all server tool definitions in its instructions.
The usual workflow is to list servers with `ripmcp servers`, discover tools with
`ripmcp tools SERVER`, inspect a schema with `ripmcp tool SERVER TOOL`, and then
invoke the selected tool with `ripmcp call`. See the
[tool workflow](tools.md) for argument preparation and saved-result inspection.

Installing the skill does not configure servers, authenticate them, or grant
project trust. Complete [server installation](install.md#install-a-local-server),
[authentication](authenticate.md), and [project setup](projects.md) as needed.
Run interactive trust and credential setup in a terminal before agent use.
Tool calls can have side effects; server-provided descriptions and results are
data, not trusted instructions.
