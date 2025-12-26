# Pavis
**The High-Performance, Memory-Safe Service Mesh Data Plane**

![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)
![Language](https://img.shields.io/badge/language-Rust-orange.svg)
![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)
![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)
[![Crates.io](https://img.shields.io/crates/v/pavis.svg)](https://crates.io/crates/pavis)

**Pavis** is a next-generation Service Mesh sidecar proxy built on **Rust** and **Cloudflare Pingora**. It replaces heavy C++ sidecars (like Envoy) with a lightweight, crash-safe alternative using a **"Split Data Plane"** architecture.

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

- **[Architecture.md](./Architecture.md)** - System design, protocol specification, and component details
- **[ROADMAP.md](./ROADMAP.md)** - Development phases and progress tracking