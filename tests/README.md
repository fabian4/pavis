# Pavis Test Suite Architecture

This document describes the test architecture for Pavis. It serves as the primary entry point for understanding how to run and write tests.

**Detailed Design Specifications:**
- [Runtime Suite Design](../docs/testing/runtime_suite.md) - Focuses on the `pavis` data plane binary.
- [Relay Suite Design](../docs/testing/relay_suite.md) - Focuses on the `pavis-relay` control plane.
- [Integrated Suite Design](../docs/testing/integrated_suite.md) - Focuses on end-to-end system integration.

---

## Overview

Pavis tests are organized into **three distinct suites**, each with a clear responsibility:

1. **Runtime Suite**: Verifies the `pavis` runtime proxy in isolation, including hot-reload and last-known-good semantics.
2. **Relay Suite**: Tests the `pavis-relay` control plane independently, focusing on fanout, long-poll, and artifact distribution.
3. **Integrated Suite**: End-to-end validation of the complete system path (pavctl → relay → runtime → upstream).

### Why Three Suites?

- **Clear Responsibility**: Each suite tests a specific component or integration boundary.
- **Faster Feedback**: Unit-level suites (runtime, relay) run faster and fail earlier than full integration tests.
- **Better Failure Isolation**: When a test fails, the suite name immediately narrows the search space.

---

## Unified Test Infrastructure

All three suites share a common foundation provided by **`pavis-testkit`**, a workspace crate containing non-production test fixtures.

### Role of `pavis-testkit`

- Provides deterministic mock services for testing.
- Ensures consistent behavior across binary, Docker, and future Kubernetes environments.
- Simplifies test authoring by abstracting environment setup.

### Testkit Binaries

`pavis-testkit` provides two binaries:

#### `pavis-mock-upstream`

- **Purpose**: Deterministic upstream HTTP/HTTPS service for testing runtime behavior.
- **Capabilities**:
  - Verifies routing, header manipulation, TLS origination.
  - Simulates failure scenarios (503s, timeouts, flakiness).
  - Exposes observability endpoints (`/received`, `/reset`) for assertions.
- **Not Production Code**: This binary exists only for testing and is never deployed.

#### `pavis-mock-relay`

- **Purpose**: Minimal mock control plane for testing runtime hot-reload without restarts.
- **Capabilities**:
  - Implements long-poll artifact distribution (opaque bytes only).
  - Provides `/publish` endpoint to inject new `.pvs` artifacts during tests.
  - Treats artifacts as opaque byte blobs (no parsing, no semantic validation).
- **Design Constraint**: The runtime hot-reload mechanism requires a live control plane. Since production `pavis-relay` is heavyweight and couples with external dependencies, `pavis-mock-relay` provides the minimal interface needed to test runtime-only semantics.

---

## Suite Boundaries

### Runtime Suite
*   **Focus**: `pavis` binary.
*   **Topology**: `pavis` + `pavis-mock-relay` + `pavis-mock-upstream`.
*   **Key Validation**: Hot reload, LKG, Routing logic, Resilience policies.

### Relay Suite
*   **Focus**: `pavis-relay` binary.
*   **Topology**: `pavis-relay` (isolated).
*   **Key Validation**: API contract, Long-poll blocking, Fanout, Persistence, Limits.

### Integrated Suite
*   **Focus**: System interaction.
*   **Topology**: `pavctl` + `pavis-relay` + `pavis` + `pavis-mock-upstream`.
*   **Key Validation**: End-to-End publishing, Real-world reload, Component compatibility.

---

## Test Environments

The same test logic can run in multiple environments:

1. **Binary Mode** (default):
   - Components are launched as native binaries.
   - Fastest execution.
   - Default for local development and CI.

2. **Docker / Docker Compose**:
   - Components run in containers.
   - Uses `pavis-testkit:local` and other local images.
   - Validates containerized deployment.

### Key Principle

Environment differences affect **how components are launched**, not **what is tested**. The same test cases run in all environments with identical assertions.

---

## Case Authoring Guide

The following sections provide detailed guidance for writing individual test cases within the three suites.

### Scope & Philosophy

An E2E case in this repository is responsible for **verifying a specific behavior of the Pavis proxy** against a controlled upstream.

### Responsibilities

- **Configure Pavis**: Provide a valid configuration (file or inline).
- **Generate Traffic**: Send HTTP/TCP requests through Pavis.
- **Assert Behavior**: Verify the response from Pavis or the state of the upstream.

### Non-Responsibilities

- **Infrastructure**: Do NOT manage Docker containers, networks, or volumes. The runner handles this.
- **Lifecycle**: Do NOT start or stop the Pavis process manually unless testing crash recovery. The harness handles the primary process.
- **Complex Logic**: Keep scripts linear. Avoid complex loops or conditionals.

### Standard Case Structure

Every case script MUST follow this canonical structure:

```bash
#!/bin/bash
# Case: [Name]
# Scenario: [Brief description of what is being tested]

# 1. Imports
source "$(dirname "$0")"/../../lib/env.sh"
source "$(dirname "$0")"/../../lib/assert.sh"

# 2. Configuration & Inputs
# Define variables or write config files relative to the case execution
setup_test "10_bootstrap"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

# 3. Execution
# Start Pavis or apply config
run_pavis ...

# 4. Traffic Generation
# Send requests with MANDATORY isolation headers
response=$(curl_pavis "/api/resource" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}" \
  -H "X-Pavis-Test-Run: ${RUN_ID}")

# 5. Assertions
# Validate HTTP status, headers, or upstream observations
assert_status 200 "$response"
```

### Mandatory Test Isolation Headers

To ensure parallel safety and determinism, **EVERY** request sent through Pavis MUST include the following headers:

1. **`X-Pavis-Test-Run`**: Identifies the specific CI run or test session.
2. **`X-Pavis-Test-Case`**: Identifies the specific test case file/function.

**Violation of this rule will cause flaky tests and CI failures.**

---

## Running Tests

Use the `run.sh` script:

```bash
# Run all suites (binary mode)
./tests/run.sh

# Run specific suite
./tests/run.sh pavis
./tests/run.sh relay
./tests/run.sh integrated

# Run specific case
./tests/run.sh pavis 10_bootstrap_static
./tests/run.sh integrated 10_bootstrap_path
```
