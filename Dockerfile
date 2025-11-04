# FROM debian:bookworm-slim

# COPY ./target/release/pdf-server ./
# RUN apt-get update && apt-get install -y libssl-dev libc6
#build step
FROM rust:1.90.0-alpine as builder
RUN apk add --no-cache \
    musl-dev \
    musl-utils \
    build-base \
    openssl-dev \
    pkgconfig \
    perl
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl

#run
FROM alpine:3.22.2
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/pdf-server . 
