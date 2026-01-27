FROM rust:1.92-slim AS builder

WORKDIR /work
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build -p pavis-benchkit --bin bench-upstream --release --locked

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /work/target/release/bench-upstream /bench-upstream

EXPOSE 8000

ENTRYPOINT ["/bench-upstream"]
