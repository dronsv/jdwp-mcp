# Trace a request path

Attach to the JVM at localhost:5005 and trace which methods are called when a request is processed.

Steps:
1. Attach to the JVM
2. Start tracing on $ARGUMENTS (the class/package pattern to trace)
3. Tell the user to send their request now (or wait a few seconds if it's automatic)
4. After a few seconds, get the trace result
5. Show the call path with method names and depth
6. If a suspicious method is found, set a breakpoint there for deeper inspection
7. Disconnect when done
