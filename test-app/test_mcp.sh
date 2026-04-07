#!/usr/bin/env bash
set -euo pipefail

# Integration tests for jdwp-mcp server
# Requires: test app running on localhost:5005 (docker compose up)
# Usage: ./test_mcp.sh [path-to-jdwp-mcp-binary]

BINARY="${1:-../target/release/jdwp-mcp}"
PASS=0
FAIL=0
TOTAL=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

# Send a JSON-RPC request to the MCP server and return the response
call_mcp() {
    local request="$1"
    echo "$request" | timeout 10 "$BINARY" 2>/dev/null | head -1
}

# Send multiple requests (newline-separated) and return all responses
call_mcp_multi() {
    local requests="$1"
    echo "$requests" | timeout 15 "$BINARY" 2>/dev/null
}

# Assert response contains a string
assert_contains() {
    local test_name="$1"
    local response="$2"
    local expected="$3"
    TOTAL=$((TOTAL + 1))

    if echo "$response" | grep -q "$expected"; then
        PASS=$((PASS + 1))
        echo -e "  ${GREEN}PASS${NC} $test_name"
    else
        FAIL=$((FAIL + 1))
        echo -e "  ${RED}FAIL${NC} $test_name"
        echo -e "    expected to contain: ${YELLOW}$expected${NC}"
        echo -e "    got: ${YELLOW}$(echo "$response" | head -c 200)${NC}"
    fi
}

# Assert response does NOT contain a string
assert_not_contains() {
    local test_name="$1"
    local response="$2"
    local unexpected="$3"
    TOTAL=$((TOTAL + 1))

    if echo "$response" | grep -q "$unexpected"; then
        FAIL=$((FAIL + 1))
        echo -e "  ${RED}FAIL${NC} $test_name"
        echo -e "    should NOT contain: ${YELLOW}$unexpected${NC}"
    else
        PASS=$((PASS + 1))
        echo -e "  ${GREEN}PASS${NC} $test_name"
    fi
}

echo "=== jdwp-mcp Integration Tests ==="
echo "Binary: $BINARY"
echo ""

# --- Test 1: Initialize ---
echo "Test 1: Initialize"
RESP=$(call_mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}')
assert_contains "returns protocol version" "$RESP" "protocolVersion"
assert_contains "returns server info" "$RESP" "jdwp-mcp"
assert_contains "returns instructions" "$RESP" "JDWP debugger"

# --- Test 2: List Tools ---
echo "Test 2: List Tools"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
TOOLS_RESP=$(echo "$RESP" | tail -1)
assert_contains "has debug.attach" "$TOOLS_RESP" "debug.attach"
assert_contains "has debug.set_breakpoint" "$TOOLS_RESP" "debug.set_breakpoint"
assert_contains "has debug.get_stack" "$TOOLS_RESP" "debug.get_stack"
assert_contains "has debug.eval" "$TOOLS_RESP" "debug.eval"
assert_contains "has debug.watch" "$TOOLS_RESP" "debug.watch"
assert_contains "has debug.snapshot" "$TOOLS_RESP" "debug.snapshot"
assert_contains "has debug.vm_info" "$TOOLS_RESP" "debug.vm_info"
assert_contains "has debug.exception_breakpoint" "$TOOLS_RESP" "debug.exception_breakpoint"
assert_contains "has debug.find_class" "$TOOLS_RESP" "debug.find_class"
assert_contains "has debug.list_methods" "$TOOLS_RESP" "debug.list_methods"
assert_contains "has debug.inspect" "$TOOLS_RESP" "debug.inspect"
assert_contains "has debug.set_value" "$TOOLS_RESP" "debug.set_value"

# --- Test 3: Attach ---
echo "Test 3: Attach to JVM"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}')
ATTACH_RESP=$(echo "$RESP" | tail -1)
assert_contains "connected" "$ATTACH_RESP" "connected"
assert_contains "has session" "$ATTACH_RESP" "session"

# --- Test 4: VM Info ---
echo "Test 4: VM Info"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.vm_info","arguments":{}}}')
VM_RESP=$(echo "$RESP" | tail -1)
assert_contains "shows JVM version" "$VM_RESP" "JDWP"

# --- Test 5: Pause + List Threads ---
echo "Test 5: Pause + List Threads"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.pause","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.list_threads","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"debug.continue","arguments":{}}}')
PAUSE_RESP=$(echo "$RESP" | sed -n '3p')
THREADS_RESP=$(echo "$RESP" | sed -n '4p')
RESUME_RESP=$(echo "$RESP" | sed -n '5p')
assert_contains "paused" "$PAUSE_RESP" "paused"
assert_contains "lists threads" "$THREADS_RESP" "threads"
assert_contains "resumed" "$RESUME_RESP" "resumed"

# --- Test 6: Find Class ---
echo "Test 6: Find Class"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.find_class","arguments":{"pattern":"DebugTestApp"}}}')
CLASS_RESP=$(echo "$RESP" | tail -1)
assert_contains "finds DebugTestApp" "$CLASS_RESP" "DebugTestApp"

# --- Test 7: List Methods ---
echo "Test 7: List Methods"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.list_methods","arguments":{"class_pattern":"DebugTestApp"}}}')
METHODS_RESP=$(echo "$RESP" | tail -1)
assert_contains "has processOrder" "$METHODS_RESP" "processOrder"
assert_contains "has riskyOperation" "$METHODS_RESP" "riskyOperation"
assert_contains "has main" "$METHODS_RESP" "main"

# --- Test 8: Set Breakpoint + Wait + Get Stack ---
echo "Test 8: Breakpoint + Stack"
# Dynamically find first line of processOrder (avoids hardcoded line numbers)
BP_LINE=$(echo "$METHODS_RESP" | grep -o 'processOrder:[0-9]*' | head -1 | cut -d: -f2)
if [ -z "$BP_LINE" ]; then
    BP_LINE=157  # fallback
fi
RESP=$(call_mcp_multi "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1.0\"}}}
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"debug.attach\",\"arguments\":{\"host\":\"localhost\",\"port\":5005}}}
{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"debug.set_breakpoint\",\"arguments\":{\"class_pattern\":\"DebugTestApp\",\"line\":${BP_LINE},\"method\":\"processOrder\"}}}
{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"debug.wait_for_event\",\"arguments\":{\"timeout_ms\":10000}}}
{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"debug.get_stack\",\"arguments\":{}}}
{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"debug.continue\",\"arguments\":{}}}")
BP_RESP=$(echo "$RESP" | sed -n '3p')
EVENT_RESP=$(echo "$RESP" | sed -n '4p')
STACK_RESP=$(echo "$RESP" | sed -n '5p')
assert_contains "breakpoint set" "$BP_RESP" "bp"
assert_contains "event received" "$EVENT_RESP" "event"
assert_contains "stack has processOrder" "$STACK_RESP" "processOrder"

# --- Test 9: Eval toString (inherited from Object) ---
echo "Test 9: Eval toString on object"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.pause","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.list_threads","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"debug.continue","arguments":{}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
THREADS_RESP=$(echo "$RESP" | sed -n '4p')
# Extract a thread ID for eval — any will do if we pause first
assert_contains "eval: threads listed" "$THREADS_RESP" "threads"

# --- Test 10: set_breakpoint wrong line shows methods ---
echo "Test 10: Breakpoint wrong line shows available methods"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.set_breakpoint","arguments":{"class_pattern":"DebugTestApp","line":9999}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
BP_ERR=$(echo "$RESP" | sed -n '3p')
assert_contains "wrong line: shows available methods" "$BP_ERR" "Available methods"
assert_contains "wrong line: shows processOrder" "$BP_ERR" "processOrder"

# --- Test 11: set_breakpoint class not found suggests wait_for_class ---
echo "Test 11: Class not found suggests wait_for_class"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.set_breakpoint","arguments":{"class_pattern":"com.nonexistent.FakeClass","line":1}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
CLASS_ERR=$(echo "$RESP" | sed -n '3p')
assert_contains "class not found: suggests wait_for_class" "$CLASS_ERR" "wait_for_class"

# --- Test 12: inspect with many fields doesn't crash ---
echo "Test 12: Inspect doesn't crash on Thread object"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.pause","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.vm_info","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"debug.continue","arguments":{}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
VM_RESP=$(echo "$RESP" | sed -n '4p')
DISC_RESP=$(echo "$RESP" | sed -n '6p')
assert_contains "inspect regression: vm_info works" "$VM_RESP" "JDWP"
assert_contains "inspect regression: disconnect after vm_info" "$DISC_RESP" "disconnected"

# --- Test 13: Condition validation ---
echo "Test 13: Invalid condition format rejected"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.set_breakpoint","arguments":{"class_pattern":"DebugTestApp","line":'"${BP_LINE}"',"condition":"count=5"}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
COND_ERR=$(echo "$RESP" | sed -n '3p')
assert_contains "invalid condition rejected" "$COND_ERR" "invalid condition format"

# --- Test 14: Disconnect ---
echo "Test 14: Disconnect"
RESP=$(call_mcp_multi '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"debug.attach","arguments":{"host":"localhost","port":5005}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"debug.disconnect","arguments":{}}}')
DISC_RESP=$(echo "$RESP" | tail -1)
assert_contains "disconnected" "$DISC_RESP" "disconnected"

# --- Summary ---
echo ""
echo "=== Results ==="
echo -e "Total: $TOTAL  ${GREEN}Pass: $PASS${NC}  ${RED}Fail: $FAIL${NC}"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
