<img src="assets/rhino.svg" alt="Pavis logo" width="96" />

# Pavis

**A Frozen Data Plane L7 Sidecar Proxy**  
_Deterministic by construction. Zero runtime policy._

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](./LICENSE)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)](https://github.com/cloudflare/pingora)

[![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)](#project-status)
[![codecov](https://codecov.io/gh/fabian4/pavis/branch/main/graph/badge.svg?token=C1DRZN5YDL)](https://codecov.io/gh/fabian4/pavis)

## Philosophy

Pavis is built around a single architectural constraint: the runtime is not allowed to interpret configuration.

All semantic work happens before deployment. The runtime only executes a fully materialized artifact.

This design is described in detail in: [**“Pavis: A Dumb Proxy for Boring Reloads”**](https://fabian4.site/blog/dumb-proxy/).

## What is Pavis?

**Pavis** is a highly opinionated L7 sidecar proxy implemented in Rust, built on the Cloudflare Pingora engine.

It acts as a runtime for executing pre-compiled, immutable policy artifacts (`.pvs`) produced by an external control plane.

At runtime, Pavis loads a versioned `.pvs` artifact and forwards traffic according to the instructions encoded in the currently active execution state.

## What is it NOT?

*   It is **NOT** a drop-in replacement for Envoy.
*   It is **NOT** a general-purpose programmable proxy.
*   It is **NOT** a platform for runtime scripting (Lua, WASM).

## Why a Frozen Data Plane?

Pavis rejects the "Smart Proxy" model. It moves complexity left—from the critical path of packet processing to the compilation phase.

*   **Immutable Execution**: The runtime executes a static binary artifact, preventing drift.
*   **No Runtime Inference**: "Missing" configuration is treated as a compile-time error rather than being filled with implicit defaults.
*   **Determinism**: By enforcing AOT compilation, resource usage and latency variance are strictly bounded.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the formal architectural invariants.

## Capabilities: Supported Today

*   **L7 Routing**: Prefix, Exact, and Regex matching.
*   **Traffic Management**: Weighted splitting, round-robin load balancing.
*   **Actions**: Forwarding, Redirects (3xx), Direct Responses (200/400/503).
*   **Header Manipulation**: Deterministic insert, remove, overwrite.
*   **Rewrites**: Prefix path and Host literal rewriting.
*   **TLS**: Server-side termination and Upstream origination.
*   **Observability**: Prometheus metrics, structured access logging, OTLP tracing.
*   **Resilience**: Active health checks, outlier ejection, circuit breaking, retries (reaction-only; no learned policy state).
*   **Hot Reload**: Atomic, hitless reload of the data plane.

See [docs/roadmap/features.md](./docs/roadmap/features.md) for a complete feature matrix.

## Operational Model

The operational model of Pavis is distinct from other proxies:

1.  **Compile**: Users compile source intent (YAML, xDS) into a `.pvs` artifact using the Codec pipeline.
2.  **Distribute**: Artifacts are versioned and distributed via the Relay.
3.  **Execute**: The Runtime pulls the artifact and atomically swaps the execution state.

For detailed operational guides (Shutdown, Admin API, Relay), see [docs/operations/runtime.md](./docs/operations/runtime.md).

## Project Status & Roadmap

**Current Status**: ⚠️ **Pre-Alpha**

The project is in active development. APIs and binary formats are subject to breaking changes.

**Planned Features:**
*   Identity (mTLS/SPIFFE)
*   RBAC (Deny-by-default)
*   xDS Compilation

See [docs/roadmap/roadmap.md](./docs/roadmap/roadmap.md) for active tracking.

## Explicitly Dropped Features

These features are excluded to preserve the Frozen Data Plane contract:
*   **No Runtime Scripting**: No WASM or Lua.
*   **No Regex Substitutions**: Matching only.
*   **No Inline Secrets**: Certificates must be file-path references.
*   **No Global Rate Limiting**: Avoids external runtime dependencies.

## Documentation

**Core Documents:**
*   **[ARCHITECTURE.md](./ARCHITECTURE.md)** - System invariants and constraints (what must never change)
*   **[docs/design.md](./docs/design.md)** - Design philosophy and architectural decisions (why & what decisions were made)

**Service Documentation:**
*   **[crates/pavis-pvs/README.md](./crates/pavis-pvs/README.md)** - PVS service (binary artifact format specification)
*   **[crates/pavis-relay/README.md](./crates/pavis-relay/README.md)** - Relay service (specifications & operations)
*   **[crates/pavis/README.md](./crates/pavis/README.md)** - Runtime service (specifications, operations & recovery)

**User Guides:**
*   **[docs/configuration/guide.md](./docs/configuration/guide.md)** - How to configure Pavis
*   **[docs/configuration/reference.md](./docs/configuration/reference.md)** - Complete configuration reference

**Project Status:**
*   **[docs/roadmap/roadmap.md](./docs/roadmap/roadmap.md)** - Development roadmap and milestones
*   **[docs/roadmap/features.md](./docs/roadmap/features.md)** - Feature status overview
*   **[docs/roadmap/coverage.md](./docs/roadmap/coverage.md)** - Test coverage analysis

**Performance:**
*   **[docs/benchmarks/methodology.md](./docs/benchmarks/methodology.md)** - How we measure and validate performance

## Repository Layout

*   `crates/pavis`: The runtime executable.
*   `crates/pavis-core`: Shared type definitions and validators.
*   `crates/pavis-codec-*`: Compilers (Source → Frozen Config).
*   `crates/pavis-relay`: Distribution control plane.
*   `crates/pavctl`: CLI for artifact generation.
