I'm debugging a Java app. Make sure it's running with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar

Attach to localhost:5005. The app throws an exception. Set an exception
breakpoint for the relevant exception class, wait for it to fire, show
the stack and all local variables at the throw site. Explain what state
caused the exception and suggest a fix.

$ARGUMENTS
