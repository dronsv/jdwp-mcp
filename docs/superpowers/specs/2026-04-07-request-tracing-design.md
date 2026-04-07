# Design: Request Tracing (debug.trace / debug.trace_result)

## Problem

When an agent modifies code in a large codebase and gets unexpected behavior,
it doesn't know which code path the request actually followed. Setting breakpoints
requires guessing where to look. Static analysis fails because large projects
have too many possible paths (DI, strategy pattern, interceptors, etc.).

## Solution

Two MCP tools that let the agent arm method-level tracing on a package/class,
send a request, then retrieve the actual call path with timing and results.

## API

### debug.trace

Starts tracing. Returns immediately.

**Input:**
- `class_pattern` (required) — package or class to trace, e.g. `com.example.service`
- `include_args` (optional, default false) — capture method arguments

**Output:** `"tracing com.example.service.* (12 classes armed)"`

**Implementation:**
- Resolve classes matching pattern via existing `resolve_classes`
- Set METHOD_ENTRY event with ClassMatch modifier, SuspendPolicy::None
- Set METHOD_EXIT event with ClassMatch modifier, SuspendPolicy::None
- Store request IDs and initialize TraceState in session

### debug.trace_result

Retrieves collected trace and optionally stops tracing.

**Input:**
- `clear` (optional, default true) — remove event requests after retrieval

**Output:**
```
trace: 8 calls, 23ms, 4 classes
#1 UserController.getUser
#2  UserService.findById
#3   PermissionChecker.check → false
#4  UserService.findById → threw AccessDeniedException
#5 ErrorHandler.handle → 404
```

With include_args:
```
#1 UserController.getUser(id=1)
#2  UserService.findById(id=1)
#3   PermissionChecker.check(user=null, role="ADMIN") → false
```

## Data Model

```rust
// Added to session.rs
struct TraceState {
    active: bool,
    entry_request_id: i32,
    exit_request_id: i32,
    include_args: bool,
    calls: Vec<TraceCall>,
    start_time: Instant,
}

struct TraceCall {
    thread_id: u64,
    class_name: String,
    method_name: String,
    depth: u32,
    entry_time_ms: u64,
    result: TraceResult,
}

enum TraceResult {
    Pending,
    Returned(Option<String>),   // formatted return value if available
    ThrewException(String),      // exception class name
}
```

## Event Processing

The event listener task already receives all events. When `trace_state.active`:

1. On METHOD_ENTRY: push TraceCall with depth++ (per thread)
2. On METHOD_EXIT: find matching pending call, set result, depth--
3. On METHOD_EXIT_WITH_RETURN_VALUE: same but capture return value

Depth tracked per-thread via HashMap<ThreadId, u32>.

## Suspend Policy

SuspendPolicy::None (0) — JVM does not pause on trace events. Critical for
performance: tracing should not change timing behavior significantly.

Note: with SuspendPolicy::None, we cannot read method arguments (that would
require the thread to be suspended). For include_args=true, we use
SuspendPolicy::EventThread (1) which suspends only the calling thread
briefly, reads args, then resumes. This is slower but still functional.

## Safety Limits

- Max 500 calls per trace. After 500, auto-clear events and append "(truncated)".
- Max 60 seconds trace duration. Auto-clear after timeout.
- trace_result returns empty if nothing was captured.

## Event Listener Changes

The conditional breakpoint evaluation already demonstrated the pattern:
extract data under brief lock, do JDWP I/O without lock. Trace events
use SuspendPolicy::None so no JDWP I/O is needed — just append to Vec
under brief lock.

For include_args (SuspendPolicy::EventThread): read args from suspended
thread using connection_clone (no session lock), then resume thread and
append result under brief lock.

## Output Format

Compact, indented by call depth. One line per method call.
Class names shortened via signature_to_display_name (existing helper).
Return values via format_value_leaf (existing helper).

## Files to Modify

| File | Change |
|------|--------|
| `mcp-server/src/session.rs` | Add TraceState, TraceCall, TraceResult |
| `mcp-server/src/handlers.rs` | Add handle_trace, handle_trace_result, update event listener |
| `mcp-server/src/tools.rs` | Add debug.trace, debug.trace_result schemas |
| `jdwp-client/src/eventrequest.rs` | Add set_class_trace (METHOD_ENTRY+EXIT with ClassMatch modifier) |

## Agent Workflow

```
1. Agent: debug.trace class_pattern=com.example.service
   → "tracing com.example.service.* (12 classes armed)"

2. Agent: sends HTTP request to application
   → gets unexpected 404

3. Agent: debug.trace_result
   → sees exact call path, finds PermissionChecker.check returned false

4. Agent: debug.set_breakpoint class=PermissionChecker line=91
   → deep-dives into the specific decision point

5. Agent: understands the issue, fixes the code
```
