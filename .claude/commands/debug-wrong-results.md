I'm debugging a Java app. Make sure it's running with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar

Attach to localhost:5005. The app returns wrong data for certain inputs.
Trace the relevant service package while I trigger the request. Show me
the call path, then set breakpoints at decision points to find where
the logic diverges from expected behavior. I need to see variable values
at each branch that determines the output.

$ARGUMENTS
