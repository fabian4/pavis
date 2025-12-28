# Pavis

**An Experimental Service Mesh Data Plane in Rust**

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](./LICENSE)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)](https://github.com/cloudflare/pingora)
[![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)](#status)
[![Crates.io](https://img.shields.io/crates/v/pavis.svg)](https://crates.io/crates/pavis)

**Pavis** is an experimental **service mesh sidecar proxy** implemented in **Rust**, built on top of **Cloudflare Pingora**.

The project explores a **split data plane** design aimed at improving **memory safety**, **operational robustness**, and **resource efficiency** compared to traditional monolithic sidecar architectures.

Pavis is primarily a **research and prototyping effort**, intended to validate architectural ideas rather than provide a production-ready replacement for existing proxies.

## Motivation

Modern service mesh sidecars are powerful but often come with significant complexity and resource overhead.  
Pavis investigates whether a Rust-based, memory-safe implementation with a more modular data plane can offer:

| Focus | Notes |
|------|------|
| 🛡️ Memory safety | Leveraging Rust to avoid common classes of memory bugs |
| 🪶 Reduced footprint | Designed with sidecar constraints in mind |
| 🔀 Split data plane | Separating control-heavy logic from the hot path |
| ⚙️ Pingora runtime | Reusing proven async networking infrastructure |

These goals are exploratory and subject to change as the project evolves.

## Status

> ⚠️ **Pre-Alpha**
>
> - APIs and on-disk formats are unstable
> - Performance characteristics are still under evaluation
> - Not intended for production use

## Development

The project uses a modular `Makefile` for common tasks.

```bash
# Build the workspace
make build

# Run unit and integration tests
make test
make test-integration

# Run full CI suite (fmt, lint, test)
make ci

# Generate API documentation
make docs

# Show all available commands
make help
```

## pavctl

`pavctl` is the primary CLI tool for managing Pavis. It is used to generate binary `.pvs` configurations, view them, and will eventually manage the proxy runtime.

```bash
# Generate a binary config from YAML (auto-names to config.pvs)
cargo run --bin pavctl -- gen config.yaml

# View a binary config
cargo run --bin pavctl -- view config.pvs
```

For performance testing, see the **[Benchmark README](./bench/README.md)**.
For E2E testing, see the **[E2E README](./crates/pavis-e2e/README.md)**.