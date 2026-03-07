FROM rust:1.91-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --features varisat-backend

FROM debian:bookworm-slim
WORKDIR /work
COPY --from=builder /app/target/release/valid /usr/local/bin/valid
COPY --from=builder /app/target/release/cargo-valid /usr/local/bin/cargo-valid
ENTRYPOINT ["valid"]
