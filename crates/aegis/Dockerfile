# Build Stage
FROM rust:1.81-slim-bookworm AS builder

# Install dependencies for Pingora/OpenSSL
RUN apt-get update && apt-get install -y \
    cmake \
    g++ \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/asgard
COPY . .

# Build only aegis
RUN cargo build --release -p aegis

# Runtime Stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/asgard/target/release/aegis /usr/local/bin/aegis

# Default config path
ENV AEGIS_CONFIG=/etc/aegis/config.yaml

ENTRYPOINT ["aegis"]
CMD ["--config", "/etc/aegis/config.yaml"]
