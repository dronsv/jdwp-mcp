# jdwp-mcp Test Plan

## Setup

```bash
cd test-app
docker compose up --build
```

Output: `DebugTestApp started. JDWP listening on port 5005.`

The app runs 3 threads:
- **deadlock-thread-1** + **deadlock-thread-2**: deadlock within ~100ms
- **worker-thread**: processes orders every 2s, throws exception every 7th iteration, modifies `status` field every 5th

## Tests

### 1. Attach + VM Info

```
Attach to localhost:5005
Show JVM info
```

Expected: connected, shows JVM version (Eclipse Temurin 21.x)

### 2. Deadlock Diagnosis (pause + thread inspection)

```
Pause the JVM and find threads that are blocked or deadlocked
```

Expected:
- Agent calls pause, list_threads, get_stack on blocked threads
- Finds deadlock-thread-1 waiting for lockB, deadlock-thread-2 waiting for lockA
- Identifies classic ABBA deadlock

### 3. Breakpoint + Variable Inspection

```
Set a breakpoint in DebugTestApp.processOrder and show me the local variables when it hits
```

Expected:
- Finds class DebugTestApp, method processOrder
- Sets breakpoint
- Waits for hit (within 2s)
- Shows locals: `order=Order{id=N, product="Widget-N", price=..., buyer=User{name="Alice"...}}`, `count=N`, `summary`, `total`, `isLargeOrder`

### 4. Object Auto-Resolve in Stack

```
Show the stack for the worker thread with full variable details
```

Expected:
- `order` resolved as `Order{id=..., product="...", price=..., buyer=@hex}`
- `buyer` shown inline as `User{name="Alice", age=30, active=true, roles=@hex}`
- Not just `@hex` for objects

### 5. Inspect Object by ID

```
Inspect the order object from the last breakpoint
```

Expected: shows all fields of Order including nested `buyer` User

### 6. Eval (method invocation)

```
Call toString() on the order object
```

Expected: returns `"Order#N(Widget-N, $X.XX)"`

```
Call getName() on the buyer object
```

Expected: returns `"Alice"`

### 7. Exception Breakpoint

```
Set an exception breakpoint for RuntimeException and wait for it
```

Expected:
- Hits within ~14s (every 7th iteration at 2s interval)
- Stack shows `riskyOperation` → `processOrder` chain
- Exception message: "Simulated error at count=N"

### 8. Field Watchpoint

```
Watch the status field of DebugTestApp for modifications
```

Expected:
- Hits within ~10s (every 5th iteration)
- Shows new value being set to "PROCESSING_BATCH_N"
- Stack shows the assignment location

### 9. Conditional Breakpoint

```
Set a breakpoint at processOrder with condition count==10
```

Expected:
- Does NOT stop on count=1..9
- Stops exactly when count=10
- Shows `count=10` in variables

### 10. Set Value

```
When stopped at a breakpoint, change the variable 'total' to 999.99
```

Expected:
- Value changes
- Subsequent code uses the new value (prints $999.99)

### 11. Snapshot

```
Take a snapshot of the current debug state
```

Expected: combined output showing last event, breakpoints, stack with variables

### 12. Find Class + List Methods

```
Find classes matching DebugTestApp
List methods of DebugTestApp
```

Expected:
- Finds DebugTestApp, DebugTestApp$User, DebugTestApp$Order
- Lists methods with line ranges: main, processOrder, riskyOperation, etc.

### 13. Step Through Code

```
Set a breakpoint at processOrder, then step over line by line
```

Expected:
- Stops at entry
- step_over advances to next line
- Variables update at each step

### 14. Disconnect

```
Disconnect from the debug session
```

Expected: disconnects cleanly, JVM resumes all threads

## Teardown

```bash
docker compose down
```

## Pass Criteria

All 14 tests produce expected results. Key validations:
- Deadlock correctly identified (test 2)
- Objects auto-resolved, not just @hex (test 4)
- Conditional breakpoint fires only on matching value (test 9)
- Exception breakpoint catches RuntimeException (test 7)
- Field watchpoint detects status modification (test 8)
