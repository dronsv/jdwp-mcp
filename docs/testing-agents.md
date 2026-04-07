# Testing and Tuning JDWP Agents & Skills

## Setup

Start the test app with known scenarios:

```bash
cd test-app
docker compose up --build -d
```

The app provides:
- **Deadlock** — two threads grab locks in opposite order (instant, permanent)
- **Worker loop** — processes orders every 2s, modifiable state
- **Exception** — throws RuntimeException every 7th iteration (~14s)
- **Field modification** — updates `status` field every 5th iteration (~10s)

## Testing slash commands

### /investigate-hang

```bash
# In Claude Code, with test app running:
/investigate-hang localhost:5005
```

**Expected behavior:**
1. Calls `debug.attach`
2. Calls `debug.pause`
3. Calls `debug.list_threads`
4. Identifies deadlock-thread-1 and deadlock-thread-2 as blocked
5. Calls `debug.get_stack` on both
6. Reports: "ABBA deadlock — thread-1 holds lockA, waits lockB; thread-2 holds lockB, waits lockA"
7. Calls `debug.disconnect`

**Red flags (tune if you see these):**
- Agent doesn't pause first → add "always pause before listing threads" to command
- Agent only checks one thread → add "check ALL blocked threads"
- Agent doesn't identify the deadlock pattern → add example of what deadlock looks like in stack output
- Agent forgets to disconnect → add "always disconnect when done"

### /investigate-exception

```bash
/investigate-exception localhost:5005
```

**Expected behavior:**
1. Calls `debug.attach`
2. Calls `debug.exception_breakpoint` (RuntimeException)
3. Calls `debug.continue` (if JVM was paused)
4. Calls `debug.wait_for_event` — hits within ~14s
5. Calls `debug.get_stack` — sees `riskyOperation` → `processOrder`
6. Reports: "RuntimeException at DebugTestApp.riskyOperation, count divisible by 7"
7. Clears breakpoint, disconnects

**Red flags:**
- Agent sets breakpoint on wrong exception class → make class more explicit in command
- Agent doesn't wait long enough → increase timeout hint
- Agent doesn't inspect variables at throw site → add "inspect local variables"

### /trace-request

```bash
/trace-request DebugTestApp
```

**Expected behavior:**
1. Calls `debug.attach`
2. Calls `debug.trace` with `class_pattern=DebugTestApp`
3. Tells user to wait (or waits 3-5s for worker loop)
4. Calls `debug.trace_result`
5. Shows: `Order.<init>` → `processOrder` → `sleep` cycle
6. Disconnects

**Red flags:**
- Agent doesn't wait between trace and trace_result → add explicit "wait 3-5 seconds"
- Trace shows 0 calls → pattern might be wrong, check class_pattern format

## Testing the investigator agent

Spawn via Claude Code:

```
Use the jdwp-investigator agent to diagnose why the app at localhost:5005 has threads stuck
```

Or directly reference:

```
@jdwp-investigator The app at localhost:5005 seems to have a deadlock. Find it.
```

**Evaluation criteria:**
1. **Correctness** — did it find the right root cause?
2. **Efficiency** — how many tool calls? (fewer = better, optimal is ~5-7 for deadlock)
3. **Completeness** — did it report all relevant info (both threads, both locks)?
4. **Cleanup** — did it disconnect?

## How to tune agent prompts

### Step 1: Run and observe

Run the command/agent, watch the tool calls in Claude Code output.

### Step 2: Identify gaps

Common issues:
| Problem | Fix in prompt |
|---------|---------------|
| Wrong tool order | Add explicit numbered steps |
| Missing step | Add the step explicitly |
| Too many tool calls | Add "prefer snapshot over separate calls" |
| Doesn't handle errors | Add "if attach fails, tell user to check JDWP flags" |
| Verbose output | Add "report concisely, not raw tool output" |
| Forgets cleanup | Add "always disconnect when done" as last step |

### Step 3: Edit and retry

Agent prompts are in:
- `.claude/agents/jdwp-investigator.md`
- `.claude/commands/investigate-hang.md`
- `.claude/commands/investigate-exception.md`
- `.claude/commands/trace-request.md`

Edit the file, run again against the same test scenario.

### Step 4: Test edge cases

After the happy path works:
- What if JVM is not running? (connection refused)
- What if no threads are blocked? (no deadlock)
- What if exception doesn't fire within timeout?
- What if trace returns 0 calls? (wrong pattern)

Add handling for these in the prompt.

## Automated agent testing (advanced)

For systematic testing, create a script that:

```bash
#!/bin/bash
# 1. Start test app
docker compose up --build -d
sleep 5

# 2. Run agent command via Claude Code SDK
claude --print "Use jdwp-investigator to find the deadlock at localhost:5005" \
  --allowedTools 'mcp__jdwp__*' \
  2>&1 | tee test-output.txt

# 3. Check output for expected findings
grep -q "deadlock" test-output.txt && echo "PASS: found deadlock" || echo "FAIL: missed deadlock"
grep -q "lockA" test-output.txt && echo "PASS: identified lockA" || echo "FAIL: missed lockA"
grep -q "disconnect" test-output.txt && echo "PASS: disconnected" || echo "FAIL: no disconnect"

# 4. Cleanup
docker compose down
```

This gives repeatable, scriptable agent evaluation.

## Metrics to track

When tuning, measure:
- **Tool calls per investigation** — target: 5-8 for simple cases
- **Time to root cause** — target: under 30s for deadlock, under 60s for exception
- **False positives** — agent claims issue that doesn't exist
- **Missed findings** — agent didn't report something obvious
- **Cleanup rate** — does it always disconnect?
