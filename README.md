# Pavis

**A Frozen Data Plane Implementation in Rust**

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](./LICENSE)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)](https://github.com/cloudflare/pingora)
[![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)](#status)
[![Crates.io](https://img.shields.io/crates/v/pavis.svg)](https://crates.io/crates/pavis)
[![codecov](https://codecov.io/gh/fabian4/pavis/branch/main/graph/badge.svg?token=C1DRZN5YDL)](https://codecov.io/gh/fabian4/pavis)

**Pavis** is a **Frozen Data Plane** implemented in **Rust**, built on top of **Cloudflare Pingora**.

It enforces a strict separation between policy resolution and packet forwarding. Unlike dynamic proxies that evaluate complex logic at runtime, Pavis executes **only** validated, immutable artifacts. All routing, security, and policy semantics are resolved, compiled, and finalized **before** deployment.

This architectural model guarantees **determinism**, **operational safety**, and **bounded resource usage** by rejecting runtime programmability.

## Architecture: Frozen Data Plane

Pavis fundamentally differs from programmable data planes (like Envoy or Nginx with Lua/WASM).

*   **Immutable Execution**: The runtime executes a static `.pvs` artifact. It does not load plugins, scripts, or WASM modules.
*   **Compile-Time Resolution**: Complex decisions (regex compilation, policy evaluation, schema validation) occur in the **Codec** stage, not on the request path.
*   **Bounded Behavior**: By removing runtime extensibility, the proxy's memory footprint and CPU latency are predictable and stable.

## Consequences of the Model

The "Frozen Data Plane" architecture dictates the feature set and operational characteristics of Pavis:

| Consequence         | Reasoning                                                                 |
| ------------------- | ------------------------------------------------------------------------- |
| 🛡️ **Memory Safety** | Logic is implemented in Rust and fixed at compile time; no JIT or unsafe script runtimes. |
| 🪶 **Minimal Footprint**| The runtime engine strips out all policy evaluation engines (Lua, WASM), retaining only forwarding logic. |
| ⚡ **Zero-Cost Abstractions** | Configuration is compiled to a zero-copy binary format (`.pvs`) optimized for direct memory mapping. |
| 🔒 **Hardened Security** | Attack surface is reduced by eliminating dynamic code execution and runtime reconfiguration logic. |

## Status

> ⚠️ **Pre-Alpha**
>
> - APIs and on-disk formats are unstable
> - Performance characteristics are still under evaluation
> - Not intended for production use