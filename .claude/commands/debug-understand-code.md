I'm debugging a Java app. Make sure it's running with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar

Attach to localhost:5005. I need to understand how this code actually
works at runtime, not just by reading source. Find the class, list its
methods with line numbers, set breakpoints at key entry points, and step
through showing me how state changes line by line. Show variable values
at each step.

$ARGUMENTS
