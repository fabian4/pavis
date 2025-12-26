# Pavis Intermediate Tests

This directory contains intermediate-level tests that bridge the gap between unit tests and E2E benchmarks.

## 1. Integration Tests (`integration.rs`)
**Command:** `make test-integration`

Validates interactions between core subsystems (Config, Router, Upstream) without full network stack initialization.

**Key Scenarios:**
- **Configuration-Driven Routing:** Ensures YAML routes map to correct upstreams.
- **Load Balancer State:** Verifies algorithms (e.g., Round Robin) update state correctly.

## 2. CLI Tests (`cli.rs`)
**Command:** `make test-cli`

Validates the `pavis` binary as a black-box executable.

**Key Scenarios:**
- **Argument Parsing:** Checks flags like `--help`, `--version`, and config paths.
- **Config Safety:** Ensures invalid configs cause exit with error.
- **Lifecycle:** Verifies graceful shutdown on signals (e.g., `SIGINT`).
