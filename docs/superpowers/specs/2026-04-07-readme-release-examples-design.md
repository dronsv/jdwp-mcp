# Design: README Rewrite, Release CI, Example Scenarios (v2)

## Goal

Make jdwp-mcp a product landing page, not developer notes. A Java developer should understand the value in 10 seconds, install in 60 seconds, and debug their first issue in 5 minutes.

## Audience

Java developers who use Claude Code. They know Java, know debugging is painful, and want to know: "will this help me or not?"

## Key Principle

README answers "why should I care?" and "how do I start?" — not "what's implemented?"

---

## Deliverables

### 1. README.md rewrite

**Order of sections:**

```
# jdwp-mcp

Hook: "Debug live JVMs through JDWP — from any MCP-compatible agent."

Badges: build status, license (MIT), latest release

Elevator pitch (3 lines):
  Attach to a running Java process, pause threads, inspect stacks and objects,
  set breakpoints, and evaluate state — through natural language.
  Works with Claude Code, Codex, Cursor, or any MCP-compatible agent.

## See it in action
  Hero scenario: hanging query, one prompt, agent finds root cause.
  Root cause stated universally: "full fact-table scan instead of aggregate path"
  (no Mondrian-specific terms in README)

## Quick Start
  ### 1. Install
    Primary: curl pre-built binary (link to releases)
    Secondary: cargo install jdwp-mcp (collapsed or below)
  ### 2. Configure
    claude mcp add jdwp ./jdwp-mcp
  ### 3. Start JVM
    java -agentlib:jdwp=... -jar app.jar
  ### 4. Debug
    One prompt example.

  Only actions, no explanations. One path. build-from-source goes to bottom.

## First prompts
  Copy-paste block of 4-5 ready prompts:
  - Attach and list threads
  - Find blocked threads
  - Set breakpoint at class:line
  - Inspect variable when breakpoint hits
  - Pause and get full stack

## Why this instead of jstack / IDE?
  3 bullets:
  - Works inside Claude Code — no tool switching
  - Combines attach + inspect + reasoning in one loop
  - Conversational: describe the problem, agent runs the debug session

## Tools
  Three compact groups, one line each:
  - Connection & control: attach, disconnect, pause, continue, step_into/over/out
  - Breakpoints & events: set_breakpoint (with conditions), exception_breakpoint,
    watch, clear_breakpoint, list_breakpoints, wait_for_event, get_last_event
  - Inspection & mutation: get_stack, get_variable, inspect, eval, set_value,
    snapshot, find_class, list_methods, vm_info, select_thread, list_threads

## Use it for
  - Hung requests and deadlocks
  - Blocked thread pools
  - Breakpoint-driven diagnosis in running services
  - State inspection when reproducing locally is hard
  - Remote debugging via kubectl port-forward

## Don't use it for
  - Postmortem heap analysis
  - Always-on observability
  - Environments where JDWP or thread pausing is operationally unsafe

## Operational note
  JDWP can change runtime behavior. Pausing threads and setting breakpoints
  may be disruptive. Use carefully in production; prefer staging or
  controlled maintenance windows.

## Examples
  Links to:
  - examples/debugging-a-hang.md (new)
  - examples/observability-debugging.md (existing)

## Architecture
  ASCII diagram + 2 lines max:
  Claude Code → MCP Server → JDWP Client → JVM
  Translates tool calls to JDWP, tracks session state,
  summarizes runtime objects.

## Building from source
  cargo build --release
  cargo test

## License
  MIT
```

**What gets removed from current README:**
- Feature list at the top (replaced by hero example)
- Status / Implemented Features checklist
- Project structure tree
- Verbose per-tool table (replaced by grouped one-liners)
- Development section details (stays in CLAUDE.md)

**What gets added (not in current README):**
- Hero scenario
- Copy-paste prompts
- "Why this instead of jstack/IDE?"
- "Use it for / Don't use it for"
- Operational safety note
- Binary-first install

### 2. Hero example: README version

Universal framing, no project-specific terms:

```
## See it in action

A database query is hanging. Find the root cause:

> Attach to localhost:5005 and find out why a query is stuck.

Claude attaches, pauses all threads, and scans for the problem:

  connected localhost:5005
  paused
  24 threads, 2 blocked

  Thread pool-3-thread-7 is waiting for a monitor lock:
  #0 RolapResult.loadMembers:142
    monitor=@3f2a  state=BLOCKED
  #1 RolapResult.execute:89

  Lock is held by pool-3-thread-2, which is running:
  #0 SqlStatement.execute:218
    sql="SELECT ... FROM fact_table"   ← full scan on 36M rows

  Root cause: the query bypassed the aggregate table and fell back to
  a full fact-table scan. Thread-7 is waiting for thread-2 to finish.
```

One prompt. Agent made 6 tool calls. Found lock contention + root cause.

### 3. Extended example: `examples/debugging-a-hang.md`

Full walkthrough (~60 lines). Here we use Mondrian-specific context:

```
# Debugging a Hanging OLAP Query

## The Problem
MDX query via XMLA endpoint takes >60s. Tomcat thread pool exhausted.

## The Session (step by step)
1. Attach to JVM
2. Pause all threads
3. List threads — find query workers
4. Select stuck thread, get stack
5. See: blocked on synchronized in RolapResult.loadMembers
6. Inspect monitor object — find which thread holds the lock
7. Get holder's stack — full table scan on mart_konfet_flat (36M rows)
8. Root cause: DistinctCountMergeFunction not configured,
   distinct-count measure bypassed agg table matching

## The Fix
Set mondrian.rolap.aggregates.DistinctCountMergeFunction=uniqCombinedMerge
in mondrian.properties.

## What the agent did
- 8 tool calls from 1 prompt
- Identified lock contention, traced holder, found SQL-level cause
- Equivalent manual work: jstack + thread dump analysis + SQL log review
```

### 4. CI Release Workflow

**File:** `.github/workflows/release.yml`

**Trigger:** Push tag `v*`

**Build matrix (5 targets):**

| Target | Runner | Rust target |
|--------|--------|-------------|
| linux-x86_64 | ubuntu-latest | x86_64-unknown-linux-musl |
| linux-aarch64 | ubuntu-latest | aarch64-unknown-linux-musl (via cross) |
| macos-x86_64 | macos-latest | x86_64-apple-darwin |
| macos-aarch64 | macos-latest | aarch64-apple-darwin |
| windows-x86_64 | windows-latest | x86_64-pc-windows-msvc |

**Asset naming:** `jdwp-mcp-<target>.tar.gz` (unix), `jdwp-mcp-<target>.zip` (windows)

**Install in README:** Link to releases page, not a brittle curl+uname one-liner. The release page itself has per-platform download links. For advanced users, a note about `cargo install`.

### 5. MCP Registry Prep

**Short description (for registry listings):**
"Debug live Java applications through natural language. Attach to any JVM via JDWP, inspect threads, stacks, objects, set breakpoints, evaluate methods — 25 tools for autonomous debugging from Claude Code."

**smithery.yaml** in repo root for smithery.ai compatibility.

**mcp.so** — submit via their process after README is live.

---

## Files to create/modify

| File | Action |
|------|--------|
| `README.md` | Rewrite |
| `examples/debugging-a-hang.md` | Create |
| `.github/workflows/release.yml` | Create |
| `smithery.yaml` | Create |
| `examples/README.md` | Update link |
