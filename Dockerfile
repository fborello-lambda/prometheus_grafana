# Stage 1: Build the app
FROM rust:slim-bullseye AS builder

WORKDIR /usr/src/api

COPY . .

RUN cargo build --release

# Stage 2: Minimal image with the binary
FROM debian:bullseye-slim

WORKDIR /usr/src/api

COPY --from=builder /usr/src/api/target/release/prometheus_grafana_hands_on .

EXPOSE 3000

CMD ["./prometheus_grafana_hands_on"]
