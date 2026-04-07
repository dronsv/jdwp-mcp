I'm debugging a Java app. Make sure it's running with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar

Attach to localhost:5005. A request is slow but doesn't hang. Trace the
relevant processing package to see which methods are called and where
time is spent. Identify the slow method, then inspect its state to find
why it's slow — large collections, expensive computations, N+1 queries,
or missing caches.

$ARGUMENTS
