# Pavis Documentation

This directory contains the canonical documentation for the Pavis project.

## Structure

### System & Architecture
- [Architecture & Invariants](architecture/invariants.md): The "Constitution" of Pavis. Immutable rules and high-level design.
- [Glossary](GLOSSARY.md): Ubiquitous language and definitions.

### Specifications (Protocol & Formats)
- [PVS Binary Format](specs/pvs_format.md): The byte-level layout of `.pvs` files.
- [Relay API](reference/API_RELAY.md): HTTP contract for the Relay.
- [Configuration Reference](reference/CONFIGURATION.md): The "Fully Materialized" RuntimeConfig reference.

### Crate-Level Documentation
Detailed design notes for specific components live with the code:
- **Runtime**: [Runtime Internals](../crates/pavis/doc/runtime_internals.md) (RCU, memory model, routing algo).
- **Relay**: [Relay Protocol](../crates/pavis-relay/doc/protocol.md) (Long-polling state machine).

### Features & Guides
- [xDS Integration](features/xds/implementation.md): Implementation plan for Envoy xDS compatibility.
- [Operations & Benchmarks](ops/benchmarks.md): Performance baselines and operational guides.

### Testing
- [Strategy](testing/STRATEGY.md): The testing pyramid and coverage rules.
- [E2E Cases](testing/e2e/): Detailed end-to-end test scenarios.
  - [Pavis Cases](testing/e2e/pavis_cases.md)
  - [Relay Cases](testing/e2e/relay_cases.md)
  - [Integrated Cases](testing/e2e/integrated_cases.md)

## Contribution Rules

1. **Architecture First**: Changes to `architecture/invariants.md` require broad consensus.
2. **Single Source of Truth**: Do not duplicate config fields or protocol specs. Link to the canonical reference.
3. **Docs as Code**: Documentation updates are required with every PR that changes behavior or APIs.
