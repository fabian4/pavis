# Pavis E2E Tests (Repository Level)

This directory hosts the repository-level End-to-End (E2E) testing system.

## Philosophy

*   **Black-Box**: Tests interact with compiled binaries only. No Rust code is shared.
*   **Shell-Orchestrated**: Bash scripts manage the test lifecycle (setup, execution, assertion, teardown).
*   **Repo-Level**: These tests sit outside the Cargo workspace to strictly enforce the boundary between "building the software" and "validating the system".

## Structure

*   `config/`: Centralized configuration fixtures for all suites.
*   `scripts/`: Core test runner and helper libraries.
*   `suites/`: Test suites organized by validation boundary.
    *   `pavis/`: Data-plane tests (routing, headers, etc.).
    *   `relay/`: Control-plane tests (ingest, distribution).
    *   `integrated/`: System-level flow (Publish -> Relay -> Pavis).

## Running Tests

```bash
# Run all suites
./e2e/scripts/run.sh

# Run a specific suite
./e2e/scripts/run.sh pavis
```