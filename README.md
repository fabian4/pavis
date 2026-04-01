<img src="assets/rhino.svg" alt="Pavis logo" width="96" />

# Pavis

## Positioning
Pavis is an engineering thesis that proves a Frozen Data Plane can run a Layer 7 proxy without any runtime interpretation. It is a compiler pipeline plus a dumb executor that only loads immutable `.pvs` artifacts. It is not a product roadmap, not a platform for feature plug-ins, and not a service mesh.

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](./LICENSE)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Engine](https://img.shields.io/badge/engine-Pingora-purple.svg)](https://github.com/cloudflare/pingora)

[![Status](https://img.shields.io/badge/status-Pre--Alpha-red.svg)](#project-status)
[![codecov](https://codecov.io/gh/fabian4/pavis/branch/main/graph/badge.svg?token=C1DRZN5YDL)](https://codecov.io/gh/fabian4/pavis)

## Core Thesis
- All routing, security, and retry semantics are compiled ahead of time into `RuntimeConfig` and then sealed into `.pvs` artifacts.
- The runtime only swaps between validated artifacts; it does not interpret text config, evaluate policy, or invent defaults.
- Failure is explicit: an artifact either loads atomically or is rejected, and the runtime keeps serving the last-known-good payload.
- The Relay remains opaque. It never inspects artifacts and only handles versioning and persistence.
- Operational recovery is limited to reloading a previously sealed artifact; there is no heuristics-based repair path.

This design is described in detail in: [**“Pavis: A Dumb Proxy for Boring Reloads”**](https://fabian4.site/blog/dumb-proxy/).

## Deliberate Non-Goals
- No runtime DSLs, WASM, Lua, or scripting of any form.
- No graceful degradation, traffic shadow heuristics, or best-effort fallbacks.
- No runtime xDS client, Kubernetes operator baked into the runtime, or gateway-layer feature surface.
- No global or local dynamic policy engines, token validation, or external auth hooks.

## What Is Closed
The compiler pipeline, artifact sealing, runtime execution, security stack, observability surface, and relay boundaries are implemented and verified under the Frozen Data Plane rule set. Capabilities are cataloged in [docs/roadmap/features.md](./docs/roadmap/features.md).

## Developer Workflow
- Use `pavctl check config.yaml` to validate a high-level config.
- Use `pavctl check --against current.pvs next.yaml` to verify that a candidate change stays within the current runtime hot-reload boundary.
- Use `pavctl gen`, `pavctl view`, and `pavctl publish` for artifact generation, inspection, and relay delivery.

## Performance Overview

Here’s a summary of the **current benchmark results** based on CI-level testing (which is limited by resources such as CPU cores and workers). 
These results reflect the **current status** and are expected to improve with optimized production environments.

Performance benchmarks are executed continuously in CI.
See the latest results here: https://github.com/fabian4/pavis/actions/workflows/pipeline.yaml

> **Note**: These performance results are based on CI testing and will vary in real-world, production environments with optimized resources.
