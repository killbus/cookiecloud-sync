# syntax=docker/dockerfile:1
FROM rust:1.96-alpine AS chef
RUN cargo install cargo-chef --version 0.1.77 --locked

FROM chef AS planner
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json
COPY src/ src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/cookiecloud-sync /app/cookiecloud-sync

FROM alpine:3.24
RUN apk add --no-cache ca-certificates
COPY --link --from=builder /app/cookiecloud-sync /cookiecloud-sync
ENTRYPOINT ["/cookiecloud-sync"]
