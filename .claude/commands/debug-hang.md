I'm debugging a Java app. Make sure it's running with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar

Attach to localhost:5005. The app is not responding. Pause all threads,
find which are blocked or waiting, show their stacks. If there's lock
contention, find who holds the lock and show their stack too.
Report the root cause.

$ARGUMENTS
