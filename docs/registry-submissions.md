# MCP Registry Submissions

Ready-to-use text for submitting jdwp-mcp to registries.

## awesome-mcp-servers PR

**Repo:** https://github.com/punkpeye/awesome-mcp-servers

**Category:** Developer Tools

**Line to add:**

```markdown
- [jdwp-mcp](https://github.com/dronsv/jdwp-mcp) - Debug live JVMs through JDWP. Attach, set breakpoints, inspect stacks/objects, evaluate methods — 25 tools for autonomous Java debugging from any MCP agent.
```

**PR title:** `Add jdwp-mcp — Java/JVM debugging via JDWP`

**PR body:**

```
Adds jdwp-mcp to the Developer Tools section.

**What it does:** MCP server that connects to live JVMs via JDWP protocol,
enabling LLM agents to autonomously debug Java applications — attach to
a running process, pause threads, set breakpoints (including conditional),
inspect stacks with auto-resolved object fields, evaluate methods, set
variable values, and diagnose issues like deadlocks and hung queries.

**25 tools** including: attach, breakpoints (conditional), exception breakpoints,
field watchpoints, stack inspection with object auto-resolve, method evaluation,
variable mutation, thread management, and combined snapshot dumps.

**Works with:** Claude Code, Codex, Cursor, or any MCP-compatible agent.

**Install:** `pip install jdwp-mcp` or `cargo install --git https://github.com/dronsv/jdwp-mcp`

**License:** MIT
```

## mcp.so

**URL:** https://mcp.so/submit

**Fields:**
- Name: jdwp-mcp
- Repository: https://github.com/dronsv/jdwp-mcp
- Short description: Debug live JVMs through JDWP — attach, breakpoints, inspect stacks/objects, evaluate methods. 25 tools for autonomous Java debugging.
- Category: Developer Tools
- Tags: java, jvm, debugging, jdwp

## smithery.ai

**URL:** https://smithery.ai/submit

`smithery.yaml` is already in the repo root. Submit the GitHub URL.

## glama.ai

Auto-indexed from GitHub topics (already set):
mcp, mcp-server, jdwp, java, debugger, llm, claude-code, model-context-protocol
