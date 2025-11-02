FROM debian:bookworm-slim

COPY ./target/release/pdf-server ./
RUN apt-get update && apt-get install -y libssl-dev
