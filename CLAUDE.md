# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

jdwp-mcp is an MCP (Model Context Protocol) server that enables LLMs to debug Java applications via JDWP (Java Debug Wire Protocol). It's a Rust workspace with two crates:

- **jdwp-client**: Low-level JDWP protocol client library (TCP connection, binary packet encoding/decoding, event loop)
- **mcp-server**: MCP server binary (`jdwp-mcp`) that translates MCP JSON-RPC over stdio into JDWP commands

## Build Commands

```bash
cargo build                # debug build
cargo build --release      # release build (binary at target/release/jdwp-mcp)
cargo test                 # run all tests
cargo test -p jdwp-client  # test just the client crate
cargo test -p jdwp-mcp     # test just the MCP server crate
```

## Architecture

```
LLM (Claude Code) --[stdio JSON-RPC]--> mcp-server --[JDWP binary over TCP]--> JVM
```

### jdwp-client crate

The client implements JDWP over an async event loop:

- `connection.rs` — `JdwpConnection`: TCP connect, JDWP handshake, entry point for sending commands
- `eventloop.rs` — `EventLoopHandle`: tokio task that multiplexes outgoing commands and incoming replies/events on one TCP stream. Commands get routed by packet ID; events go to an mpsc channel. Commands can be sent from multiple tasks; events should be consumed by one.
- `protocol.rs` — `CommandPacket`/`ReplyPacket`: binary encoding/decoding (big-endian, architecture-independent)
- `commands.rs` — JDWP command set/command constants (mirrors the spec)
- `types.rs` — `Value`, `Location`, `Variable`, type tags
- `events.rs` — Event parsing (breakpoint, step, thread start/death, VM death)
- `vm.rs`, `thread.rs`, `method.rs`, `reftype.rs`, `stackframe.rs`, `object.rs`, `string.rs` — Higher-level command wrappers organized by JDWP command set
- `reader.rs` — Cursor-based reader for JDWP data with variable-width ID fields
- `eventrequest.rs` — Event request building (breakpoints, stepping)

### mcp-server crate

- `main.rs` — Stdio transport loop: reads JSON-RPC lines from stdin, dispatches to handler, writes responses to stdout. Tracing goes to stderr only.
- `protocol.rs` — MCP/JSON-RPC type definitions (request, response, notification, tool schema)
- `tools.rs` — Tool schema definitions (16 tools: attach, breakpoints, stepping, stack/variable inspection, thread management, event waiting)
- `handlers.rs` — Request routing and tool execution. Contains class name resolution logic (JVM signatures like `Lcom/example/Foo;` to dot-notation). This is the largest file.
- `session.rs` — `SessionManager`/`DebugSession`: tracks active JDWP connection, breakpoints, threads, selected thread, last event

### Key Design Decisions

- **Localhost-only by default**: `debug.attach` requires `allow_remote: true` to connect to non-localhost hosts
- **Single event consumer**: The event loop's `EventLoopHandle` can be cloned for commands but only one consumer should call `recv_event()`
- **Smart summarization**: Stack/variable inspection truncates large objects to avoid overwhelming LLM context
- **No external UUID crate**: Session IDs use a simple timestamp+counter generator in `session.rs`

## Testing

Integration tests require a running JVM with JDWP enabled. Example test targets live in `examples/` and are registered as cargo examples on the `jdwp-client` crate:

```bash
# Start a test JVM first:
java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar myapp.jar

# Run an example:
cargo run -p jdwp-client --example test_connection
cargo run -p jdwp-client --example test_breakpoint
cargo run -p jdwp-client --example test_manual_stack
```

Unit tests (packet encoding, ID generation) run without a JVM:
```bash
cargo test
```

## MCP Server Usage

Register with Claude Code:
```bash
claude mcp add --scope project jdwp /path/to/target/release/jdwp-mcp
```

Or via `.mcp.json`:
```json
{
  "mcpServers": {
    "jdwp": {
      "command": "/path/to/target/release/jdwp-mcp"
    }
  }
}
```
