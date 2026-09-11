# ripmcp documentation

ripmcp manages local and remote MCP servers from the shell. Discover tools, inspect
their inputs, invoke them once, and inspect saved JSON without another invocation.

For a standalone terminal reference, use `man 1 ripmcp` or read its
[Markdown source](reference/manual.md). See [manual installation](how-to/install.md#install-the-manual)
if the page is not installed.

**Start here:** [Build ripmcp](how-to/install.md), then follow
[your first tool call](tutorials/first-call.md). To learn result inspection without
a server or credentials, use the [offline tutorial](tutorials/inspect-json.md).

This documentation describes the repository implementation, not the historical
proposals. It supports Linux and targets MCP `2026-07-28` only. Check
[compatibility and validation limits](reference/compatibility.md) before choosing
a server; an MCP label alone does not establish compatibility.

## Choose what you need

| I want to...           | Read                                                                                                                                                                                                                                          |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Learn by doing         | **Tutorials:** [first call](tutorials/first-call.md), [inspect JSON](tutorials/inspect-json.md)                                                                                                                                               |
| Complete a task        | **How-to guides:** [installation](how-to/install.md), [authentication](how-to/authenticate.md), [project setup](how-to/projects.md), [tool workflow](how-to/tools.md), [removal](how-to/remove.md), [troubleshooting](how-to/troubleshoot.md) |
| Look up exact behavior | **Reference:** [commands](reference/cli.md), [configuration](reference/configuration.md), [output and limits](reference/output.md), [compatibility](reference/compatibility.md)                                                               |
| Understand the design  | **Explanation:** [lifecycle and safety](explanation/lifecycle-and-safety.md)                                                                                                                                                                  |

These four sections follow Diataxis. Start with a short workflow; follow its links
for options, exact contracts, or design details instead of reading everything first.

## Working on ripmcp

Use the [source and validation map](reference/development.md) to check documentation
against code and tests. [Archived ideas and implementation notes](old/index.md)
preserve the design history; they are not the current user guide.
