ARG APP_NAME=duckchess-edge-service
ARG DIRECTORY=edge-service

FROM clux/muslrust:stable AS builder
ARG DIRECTORY
ARG APP_NAME
WORKDIR /app
COPY . .
WORKDIR ${DIRECTORY}
RUN cargo build --release

FROM alpine:3.23
ARG DIRECTORY
ARG APP_NAME
COPY --from=builder /app/${DIRECTORY}/target/x86_64-unknown-linux-musl/release/${APP_NAME} /app/${APP_NAME}
COPY ${DIRECTORY}/start.sh /app/start.sh
RUN chmod +x /app/start.sh

ENV BINARY_PATH=/app/${APP_NAME}

CMD ["/app/start.sh"]
