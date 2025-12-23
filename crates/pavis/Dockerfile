# syntax=docker/dockerfile:1.7

FROM lukemathwalker/cargo-chef:latest-rust-1.84 AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y \
    cmake \
    g++ \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=/app/target \
    cargo build --release -p pavis && \
    cp /app/target/release/pavis /tmp/pavis_tmp

FROM builder AS final

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /tmp/pavis_tmp /usr/local/bin/pavis

ENV PAVIS_CONFIG=/etc/pavis/config.yaml

ENTRYPOINT ["pavis"]
CMD ["--config", "/etc/pavis/config.yaml"]