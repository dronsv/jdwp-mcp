# Investigate a hanging JVM

Attach to the JVM at $ARGUMENTS (default: localhost:5005) and diagnose why it's hanging.

Steps:
1. Attach to the JVM
2. Pause all threads
3. List threads and identify which are blocked or waiting
4. Get the stack for each blocked thread
5. If a thread is waiting on a lock, find who holds it and get their stack
6. Summarize: what's stuck, what's blocking it, and why
7. Resume and disconnect

Use snapshot for a quick initial view. Report the root cause concisely.
