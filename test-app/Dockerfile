FROM eclipse-temurin:21-jdk-alpine

WORKDIR /app
COPY src/DebugTestApp.java .
RUN javac DebugTestApp.java

EXPOSE 5005

CMD ["java", \
     "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005", \
     "DebugTestApp"]
