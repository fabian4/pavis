# Contributing to Pavis

Thank you for your interest in contributing to Pavis.

Pavis is a highly opinionated project built around the **Frozen Data Plane**
architecture. Contributions are welcome, but must strictly respect the
architectural boundaries described below.

## Before You Start

Please read the following documents before making any changes:

- ARCHITECTURE.md
- docs/FEATURES.md
- AGENTS.md (for AI-assisted contributions)

PRs that do not align with these documents are unlikely to be accepted.

## Architectural Constraints (Non-Negotiable)

The following rules define the core design of Pavis and must not be violated:

- The runtime (`crates/pavis`) must remain a pure execution engine.
  - No defaults
  - No intent interpretation
  - No semantic validation
  - No scripting or plugin systems

- All policy resolution and validation must occur in:
  - Codec layers
  - Validation logic (`pavis-core`)

- Core data structures must:
  - Be strongly typed
  - Avoid `Option<T>` where possible
  - Remain compatible with zero-copy serialization (`rkyv`)

## What We Accept

We welcome contributions that:

- Fix bugs
- Improve performance
- Improve documentation
- Add tests that preserve architectural boundaries

We are generally cautious about changes that increase runtime flexibility at
the cost of determinism or safety.

## Submitting a Pull Request

Before submitting a PR, please ensure:

- `make ci-local` passes

All contributions are reviewed on a best-effort basis.
