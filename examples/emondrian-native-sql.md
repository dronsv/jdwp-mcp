# eMondrian Example: Debugging NativeSqlCalc Routing

This example is specific to the `emondrian_changes` project and focuses on the WD routing path:

1. Start Tomcat/eMondrian with JDWP enabled.
2. Attach the debugger to `localhost:5005`.
3. Set breakpoints at:
   - `mondrian.rolap.RolapMemberCalculation` near `getCompiledExpression`
   - `mondrian.rolap.NativeSqlRegistry` near `tryCreateCalc`
   - `mondrian.rolap.NativeSqlCalc` near `evaluate`
4. Trigger the problematic MDX query.
5. Use the debugger to wait for events and inspect routing state.

Suggested prompts:

```text
Attach to localhost:5005
Set a breakpoint at mondrian.rolap.RolapMemberCalculation line 67
Set a breakpoint at mondrian.rolap.NativeSqlRegistry line 31
Set a breakpoint at mondrian.rolap.NativeSqlCalc line 126
Wait for the next breakpoint event
Show me the current stack with variables
Get variable evaluator from the current frame
Get variable baseCube from the current frame
Get variable resolvedAxisHierarchies from the current frame
```

Useful follow-up prompts:

```text
Get variable axisBindings from frame 0
Get variable query from frame 1
Get variable session from frame 0
Show me the current stack with variables
Get the last event
```
