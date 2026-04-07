# Debugging a Hanging OLAP Query

A real debugging session using jdwp-mcp to diagnose a Mondrian OLAP engine hang.

## The Problem

An MDX query sent via XMLA endpoint takes over 60 seconds. The Tomcat thread pool
is exhausted, and the application stops responding to new requests.

Traditional approach: run `jstack`, save the output, manually search for blocked
threads, cross-reference lock owners, find the SQL, check the query plan. Takes
15-30 minutes of focused work.

With jdwp-mcp: one prompt.

## The Session

### Step 1: Attach and pause

```
> Attach to localhost:5005 and find out why queries are hanging.
```

The agent connects and immediately pauses all threads to get a consistent snapshot:

```
connected localhost:5005 session=session_1
paused
```

### Step 2: Find stuck threads

The agent lists all 24 threads and identifies two that are blocked:

```
24 threads:
0x7f01 running
0x7f02 running
0x7f03 suspended *event
...
0x8a12 suspended  ← pool-3-thread-7
0x8a13 suspended  ← pool-3-thread-2
```

### Step 3: Inspect the blocked thread

```
thread 0x8a12, 8 frames:
#0 RolapResult.loadMembers:142
  monitor=@3f2a  state=BLOCKED (waiting for lock)
#1 RolapResult.execute:89
  query=MDX{...}
#2 RolapConnection.execute:165
  statement=@4b1c
...
```

Thread-7 is stuck at `loadMembers:142`, waiting on a monitor lock (`@3f2a`).

### Step 4: Find who holds the lock

The agent inspects the monitor object and traces it to pool-3-thread-2:

```
> Who holds the lock at @3f2a?
```

```
thread 0x8a13, 12 frames:
#0 SqlStatement.execute:218
  sql="SELECT sku_unified_id, period_month, akb_state, sku_count_state
       FROM mart_konfet_flat
       WHERE period_month = '2024-01'"
#1 RolapStar.query:445
  aggStar=null  ← no aggregate table selected
#2 AggregationManager.findAgg:128
  rollableLevelBitKey={}  ← empty, all zeros
...
```

### Step 5: Root cause

Thread-2 is executing a full table scan on `mart_konfet_flat` (36M rows).

The stack shows `aggStar=null` — no aggregate table was selected. The
`rollableLevelBitKey` is empty (`{}`), which means the aggregation manager
refused to route this query to any aggregate table.

This happens because the measure uses `distinct-count`, and Mondrian's default
behavior sets `rollableLevelBitKey` to all-zeros for distinct-count measures,
blocking aggregate table matching entirely.

### Step 6: The fix

Set the merge function property in `mondrian.properties`:

```properties
mondrian.rolap.aggregates.DistinctCountMergeFunction=uniqCombinedMerge
mondrian.rolap.aggregates.DistinctCountMergeColumns=akb_state,sku_count_state
```

This tells the aggregation manager to route distinct-count measures through
HyperLogLog merge functions, enabling aggregate table matching.

After the fix: query time drops from 60+ seconds to under 2 seconds.

## What the agent did

| Step | Tool | What it found |
|------|------|---------------|
| 1 | `attach` + `pause` | Connected, froze state |
| 2 | `list_threads` | 24 threads, 2 blocked |
| 3 | `get_stack` (thread-7) | Blocked on monitor lock |
| 4 | `inspect` + `get_stack` (thread-2) | Lock holder doing full table scan |
| 5 | `get_variable` (aggStar, rollableLevelBitKey) | No agg table selected |

5 minutes of conversation replaced:
- `jstack` + manual thread dump analysis
- Cross-referencing lock IDs across threads
- Tracing SQL back to the aggregation decision
- Reading Mondrian source code to understand `rollableLevelBitKey`

## Key takeaways

- **Use `pause` + `list_threads` to diagnose hangs** — gives you a consistent snapshot
- **Stack inspection shows lock contention immediately** — no need to parse raw thread dumps
- **Variable inspection reveals the "why"** — seeing `aggStar=null` directly is faster than reasoning about it from logs
- **One conversation, not five tools** — attach, inspect, trace, diagnose in a single session
