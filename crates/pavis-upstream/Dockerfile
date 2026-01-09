# syntax=docker/dockerfile:1.6

FROM rust:1.75 as builder
WORKDIR /workspace

# Pre-copy manifests to leverage Docker layer caching when deps change infrequently.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY make ./make
COPY bench ./bench
COPY docs ./docs
COPY README.md ./README.md

RUN cargo build --locked --release -p pavis-upstream

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/pavis-upstream /usr/local/bin/pavis-upstream

ENV HTTP_PORT=8080 \
    HTTPS_PORT=8443 \
    INSTANCE_ID=pavis-upstream \
    TLS_CERT_FILE=/certs/upstream.crt \
    TLS_KEY_FILE=/certs/upstream.key

EXPOSE 8080 8443

ENTRYPOINT ["/usr/local/bin/pavis-upstream"]
