FROM rust:1.92-slim AS builder

WORKDIR /work
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build -p pavis-benchkit --bin bench-upstream --release --locked

FROM gcr.io/distroless/base-debian12

COPY --from=builder /work/target/release/bench-upstream /bench-upstream

EXPOSE 8000

ENTRYPOINT ["/bench-upstream"]
