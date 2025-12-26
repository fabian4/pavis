# Pavis

**High-Performance, Memory-Safe Service Mesh Data Plane**

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](./LICENSE)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)](https://github.com/cloudflare/pingora)
[![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)](#status)
[![Crates.io](https://img.shields.io/crates/v/pavis.svg)](https://crates.io/crates/pavis)

**Pavis** is an experimental, next-generation **Service Mesh sidecar proxy** built on **Rust** and **Cloudflare Pingora**. It explores a **Split Data Plane** architecture to replace traditional monolithic C++ sidecars (e.g., Envoy) with a **lighter, memory-safe, and crash-resilient** alternative.

## Why Pavis?

| Feature | Description |
|---------|-------------|
| 🛡️ **Memory Safety** | Rust eliminates entire classes of memory corruption issues |
| 🪶 **Minimal Footprint** | Optimized for sidecar and resource-constrained environments |
| 🔀 **Split Data Plane** | Separates control-heavy logic from the hot data path |
| ⚡ **Pingora Runtime** | Battle-tested async networking primitives from Cloudflare |

## Status

> ⚠️ **Pre-Alpha** – APIs, behavior, and performance characteristics are still evolving.

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Local development workflow
cargo run -p pavis-cli -- compile -i config.yaml -o config.pvs
cargo run -p pavis -- --config config.pvs
```

## Documentation

| Document | Description |
|----------|-------------|
| [Architecture](./Architecture.md) | System design, protocol specification, component details |
| [Roadmap](doc/ROADMAP.md) | Development phases and progress tracking |
| [Benchmarks](./bench/BENCHMARKS.md) | Performance comparison with Envoy, Nginx, HAProxy |