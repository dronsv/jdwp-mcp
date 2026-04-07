FROM eclipse-temurin:21-jdk-alpine AS build
WORKDIR /app
COPY src/DebugTestApp.java .
RUN javac DebugTestApp.java

FROM eclipse-temurin:21-jre-alpine
WORKDIR /app
COPY --from=build /app/*.class .
EXPOSE 5005
CMD ["java", \
     "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005", \
     "DebugTestApp"]
