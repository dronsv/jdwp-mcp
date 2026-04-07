# jdwp-mcp

**Debug live JVMs through JDWP — from any MCP-compatible agent.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Attach to a running Java process, pause threads, inspect stacks and objects,
set breakpoints, and evaluate state — through natural language.
Works with Claude Code, Codex, Cursor, or any MCP-compatible agent.

## See it in action

A database query is hanging. Find the root cause:

```
> Attach to localhost:5005 and find out why a query is stuck.
```

The agent attaches, pauses all threads, and scans for the problem:

```
connected localhost:5005
paused
24 threads, 2 blocked

Thread pool-3-thread-7 is waiting for a monitor lock:
#0 RolapResult.loadMembers:142
  monitor=@3f2a  state=BLOCKED
#1 RolapResult.execute:89

Lock is held by pool-3-thread-2, which is running:
#0 SqlStatement.execute:218
  sql="SELECT ... FROM fact_table"   -- full scan on 36M rows

Root cause: the query bypassed the aggregate table and fell back to
a full fact-table scan. Thread-7 is waiting for thread-2 to finish.
```

One prompt. Six tool calls. Lock contention + root cause identified.

## Quick Start

### 1. Install

```bash
curl -fsSL https://raw.githubusercontent.com/navicore/jdwp-mcp/main/install.sh | sh
```

<details>
<summary>Other install methods</summary>

```bash
# pip (no Rust required)
pip install jdwp-mcp

# cargo
cargo install --git https://github.com/navicore/jdwp-mcp

# from source
git clone https://github.com/navicore/jdwp-mcp && cd jdwp-mcp && cargo build --release
```

</details>

### 2. Configure your agent

```bash
# If installed via cargo install:
claude mcp add jdwp jdwp-mcp

# If built from source:
claude mcp add jdwp /path/to/jdwp-mcp/target/release/jdwp-mcp
```

Works with any MCP-compatible agent. For non-Claude agents, add to `.mcp.json`:

```json
{
  "mcpServers": {
    "jdwp": {
      "command": "jdwp-mcp"
    }
  }
}
```

### 3. Start your Java app with JDWP

```bash
java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar
```

### 4. Debug

```
Attach to localhost:5005 and set a breakpoint at com.example.MyService line 42
```

## First prompts

Copy-paste these to get started:

```
Attach to localhost:5005
List all threads and show which are blocked
Set a breakpoint at com.example.MyController line 65
When the breakpoint hits, show the stack and all variables
Pause the JVM and find threads waiting on locks
```

## Why this instead of jstack or an IDE?

- **Works inside your agent** — no tool switching, no separate debugger window
- **Combines attach + inspect + reasoning in one loop** — the agent decides what to look at next
- **Conversational** — describe the problem, the agent runs the debug session
- **Ground truth for large codebases** — in complex projects with deep framework stacks (Spring, Hibernate, OLAP engines), agents can get lost tracing code paths statically. Live debugging gives the agent actual runtime state instead of guesses — which thread holds the lock, what SQL was generated, what value a variable actually has right now

## Tools

25 tools organized in three groups:

**Connection and control**
`attach` `disconnect` `pause` `continue` `step_into` `step_over` `step_out`

**Breakpoints and events**
`set_breakpoint` (with optional conditions) `clear_breakpoint` `list_breakpoints` `exception_breakpoint` `watch` (field modification) `wait_for_event` `get_last_event`

**Inspection and mutation**
`get_stack` (auto-resolves objects) `get_variable` `inspect` `eval` `set_value` `snapshot` `find_class` `list_methods` `list_threads` `select_thread` `vm_info`

## Use it for

- Hung requests and deadlocks
- Blocked thread pools
- Breakpoint-driven diagnosis in running services
- State inspection when reproducing locally is hard
- Remote debugging via `kubectl port-forward`

## Don't use it for

- Postmortem heap analysis
- Always-on production observability
- Environments where JDWP attach or thread pausing is operationally unsafe

## Deploy scenarios

See [docs/deploy.md](docs/deploy.md) for setup with Maven, Gradle, Tomcat, Docker,
Kubernetes (port-forward), and SSH tunnels.

## Operational note

JDWP changes runtime behavior. Pausing threads and setting breakpoints may be
disruptive. Use carefully in production; prefer staging environments or
controlled maintenance windows.

## Examples

- [Debugging a hanging query](examples/debugging-a-hang.md) — full walkthrough of diagnosing lock contention and missing aggregate routing
- [Observability debugging](examples/observability-debugging.md) — investigating Spring Boot ObservationRegistry issues

## Architecture

```
Agent  -->  MCP Server  -->  JDWP Client  -->  TCP  -->  JVM
              |
        Translates tool calls to JDWP,
        tracks session state, summarizes
        runtime objects for the agent.
```

Core crates: `jdwp-client` (JDWP protocol) and `mcp-server` (MCP transport and session management).

## Building from source

```bash
cargo build --release
cargo test
```

## License

MIT
