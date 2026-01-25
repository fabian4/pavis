# syntax=docker/dockerfile:1.7

FROM lukemathwalker/cargo-chef:latest-rust-1.92 AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BINARY=pavis-mock-upstream

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    rm -f /var/lib/apt/lists/lock /var/cache/apt/archives/lock && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
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
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p pavis-testkit --bin ${BINARY} && \
    cp /app/target/release/${BINARY} /tmp/binary_tmp

FROM debian:bookworm-slim AS launcher
WORKDIR /src

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    rm -f /var/lib/apt/lists/lock /var/cache/apt/archives/lock && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
      gcc \
      libc6-dev \
    && rm -rf /var/lib/apt/lists/*

COPY crates/pavis-testkit/entrypoint.c /src/entrypoint.c
RUN gcc -O2 -s -o /tmp/entrypoint /src/entrypoint.c

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ARG BINARY=pavis-mock-upstream

COPY --from=builder /tmp/binary_tmp /usr/local/bin/${BINARY}
COPY --from=launcher /tmp/entrypoint /usr/local/bin/entrypoint

ENV BINARY_NAME=${BINARY}

ENTRYPOINT ["/usr/local/bin/entrypoint"]
