# Deploy Scenarios

How to enable JDWP debugging in common Java setups.

## Local development

### Maven (Spring Boot)

```bash
mvn spring-boot:run -Dspring-boot.run.jvmArguments="-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
```

### Gradle (Spring Boot)

```groovy
// build.gradle
bootRun {
    jvmArgs = ["-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"]
}
```

```bash
gradle bootRun
```

### Plain JAR

```bash
java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar
```

### Tomcat (standalone)

Add to `bin/setenv.sh`:

```bash
CATALINA_OPTS="$CATALINA_OPTS -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
```

## Docker

### Dockerfile

```dockerfile
ENV JAVA_TOOL_OPTIONS="-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
EXPOSE 5005
```

### docker-compose

```yaml
services:
  app:
    image: my-java-app
    ports:
      - "5005:5005"
    environment:
      JAVA_TOOL_OPTIONS: "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
```

Then from your host:

```
> Attach to localhost:5005
```

## Kubernetes

### Port-forward (simplest)

```bash
# Terminal 1: forward JDWP port from pod
kubectl port-forward pod/my-app-pod 5005:5005

# Terminal 2: debug
> Attach to localhost:5005
```

### Deployment with JDWP enabled

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app-debug
spec:
  replicas: 1
  template:
    spec:
      containers:
        - name: app
          image: my-java-app
          ports:
            - containerPort: 8080
            - containerPort: 5005
              name: jdwp
          env:
            - name: JAVA_TOOL_OPTIONS
              value: "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
```

Then port-forward and attach as above.

### Temporary debug pod

For quick one-off debugging without modifying the deployment:

```bash
# Override entrypoint with JDWP enabled
kubectl debug pod/my-app-pod --image=my-java-app --target=app \
  -- java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar
```

## Remote servers (SSH)

```bash
# Terminal 1: SSH tunnel
ssh -L 5005:localhost:5005 user@server

# Terminal 2: debug
> Attach to localhost:5005
```

The JVM only needs to listen on localhost — the SSH tunnel handles the rest.

## Security notes

- JDWP provides **full access** to the JVM — equivalent to running arbitrary code. Never expose port 5005 to untrusted networks.
- In production, use `address=127.0.0.1:5005` (not `*:5005`) and access via SSH tunnel or port-forward.
- jdwp-mcp defaults to **localhost-only** attach. Remote attach requires explicit `allow_remote=true`.
- Remove JDWP flags after debugging. `JAVA_TOOL_OPTIONS` is especially easy to forget.
