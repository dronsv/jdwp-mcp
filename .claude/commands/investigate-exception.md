# Investigate a Java exception

Attach to the JVM at $ARGUMENTS (default: localhost:5005) and catch the exception live.

Steps:
1. Attach to the JVM
2. Set an exception breakpoint (use the exception class from the user's stack trace, or RuntimeException if not specified)
3. Resume and wait for the exception to fire
4. When it hits, get the stack with all variables at the throw site
5. Inspect any relevant objects to understand the state that caused the exception
6. Summarize: what was thrown, where, what state caused it, and how to fix it
7. Clear the breakpoint, resume, and disconnect
