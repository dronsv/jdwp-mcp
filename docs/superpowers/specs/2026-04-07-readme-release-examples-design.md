# Design: README Rewrite, Release CI, Example Scenarios

## Goal

Make the jdwp-mcp repository compelling for Java developers discovering it through MCP registries or GitHub. A developer should understand the value in 10 seconds, install in 60 seconds, and debug their first issue in 5 minutes.

## Audience

Java developers who use Claude Code and want to add live debugging capability. They know Java, know what JDWP is conceptually, but don't want to learn a new tool's internals.

## Deliverables

### 1. README.md rewrite

**Structure (top to bottom):**

```
# jdwp-mcp

One-line hook: "Debug Java with natural language"

Badges: build status, license (MIT), latest release

Elevator pitch (3 lines): what it is, what it does, 25 tools, zero UI.

## See it in action
  Hero example: OLAP query hanging scenario (~8 lines of agent dialogue)
  Shows: one prompt → agent attaches, pauses, finds stuck thread,
  identifies lock contention + root cause.

## Quick Start
  ### 1. Install
    Option A: curl pre-built binary (linux/mac/win)
    Option B: cargo install jdwp-mcp
  ### 2. Configure Claude Code
    claude mcp add jdwp ./jdwp-mcp
  ### 3. Start Java app with JDWP
    java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar
  ### 4. Debug
    "Attach to localhost:5005 and set a breakpoint at MyService line 42"

## Tools
  Three groups, compact list format (no verbose table):
  - Connection & Control: attach, disconnect, pause, continue, step_into/over/out
  - Breakpoints & Events: set_breakpoint (with conditions), exception_breakpoint,
    watch (field modification), clear_breakpoint, list_breakpoints,
    wait_for_event, get_last_event
  - Inspection & Mutation: get_stack (auto-resolves objects), get_variable,
    inspect, eval, set_value, snapshot, find_class, list_methods, vm_info

## Examples
  Links to:
  - examples/debugging-a-hang.md (new)
  - examples/observability-debugging.md (existing)

## Architecture
  Keep existing ASCII diagram, trim to 3 lines.

## Building from source
  cargo build --release

## License
  MIT
```

**What gets removed from current README:**
- Verbose tool table with per-tool descriptions (replaced by grouped list)
- "Status" section with checkboxes (project is usable, no need for roadmap in README)
- Detailed project structure listing (belongs in CLAUDE.md, which already has it)
- Duplicate Quick Start that's scattered across sections

**Tone:** Direct, concise, confident. No emoji. No "you can" / "you might want to" — imperative voice.

### 2. Hero example scenario

**In README (short version, ~15 lines):**

Based on the Mondrian/eMondrian project context. Scenario: an OLAP query hangs in production.

```
User prompt: "OLAP query is hanging. Attach to localhost:5005 and find out why."

Agent response shows:
1. Attaches, pauses all threads
2. Lists threads, identifies pool-3-thread-7 as stuck
3. Gets stack trace showing RolapResult.loadMembers blocked on monitor
4. Identifies: thread-7 waiting on lock held by thread-2
5. Thread-2 is doing full table scan on 36M-row fact table
6. Root cause: measure bypassed agg table because DistinctCountMergeFunction not configured
```

The key: agent does 5-6 tool calls autonomously from a single prompt. Shows the compound value.

### 3. Extended example: `examples/debugging-a-hang.md`

Full walkthrough (~60 lines):

```
# Debugging a Hanging Query

## The Problem
OLAP query via XMLA takes >60 seconds, UI shows spinner.

## The Session
Step-by-step with actual tool calls and responses:
1. Attach
2. Pause all threads
3. List threads — identify query threads
4. Select stuck thread
5. Get stack — see lock contention
6. Inspect lock object — find holder thread
7. Get holder's stack — see full table scan
8. Root cause identified

## The Fix
Configure DistinctCountMergeFunction property to enable agg table routing.

## Key Takeaways
- Use pause + list_threads to diagnose hangs
- Stack inspection shows lock contention immediately
- One conversation replaced: jstack + thread dump analysis + code review
```

### 4. CI Release Workflow

**File:** `.github/workflows/release.yml`

**Trigger:** Push tag matching `v*` (e.g., `v0.2.0`)

**Build matrix (5 targets):**

| Target | Runner | Rust target | Notes |
|--------|--------|-------------|-------|
| linux-x86_64 | ubuntu-latest | x86_64-unknown-linux-musl | Static linking via musl |
| linux-aarch64 | ubuntu-latest | aarch64-unknown-linux-musl | Cross-compile via `cross` |
| macos-x86_64 | macos-latest | x86_64-apple-darwin | |
| macos-aarch64 | macos-latest | aarch64-apple-darwin | |
| windows-x86_64 | windows-latest | x86_64-pc-windows-msvc | |

**Steps per target:**
1. Checkout
2. Install Rust toolchain + target
3. Install `cross` (for linux-aarch64 only)
4. `cargo build --release --target $TARGET` (or `cross build` for cross-compilation)
5. Package: `tar.gz` for unix, `zip` for windows
6. Name format: `jdwp-mcp-{version}-{target}.{ext}`

**Release step:**
- Uses `softprops/action-gh-release` to create GitHub Release from tag
- Uploads all 5 archives as release assets
- Generates changelog from commits since last tag

**Install script in README:**
```bash
# Linux/macOS
curl -fsSL https://github.com/navicore/jdwp-mcp/releases/latest/download/jdwp-mcp-$(uname -s | tr A-Z a-z)-$(uname -m).tar.gz | tar xz
sudo mv jdwp-mcp /usr/local/bin/
```

### 5. MCP Registry Listings

**mcp.so:**
- Submit via their GitHub-based process or web form
- Needs: repo URL, short description, category (development-tools)
- Description: "Debug live Java applications through natural language. Attach to any JVM, set breakpoints, inspect objects, evaluate methods — 25 tools for autonomous debugging."

**smithery.ai:**
- Requires `smithery.yaml` in repo root with server metadata
- Will create this config file

Both registries pull README content, so the README rewrite is the primary deliverable for registry presence.

## Out of scope

- npm/npx wrapper (future consideration)
- Docker image
- Homebrew formula
- Video demo / GIF recording

## Files to create/modify

| File | Action |
|------|--------|
| `README.md` | Rewrite |
| `examples/debugging-a-hang.md` | Create |
| `.github/workflows/release.yml` | Create |
| `smithery.yaml` | Create (for smithery.ai registry) |
| `examples/README.md` | Update (add link to new example) |
