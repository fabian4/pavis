# Pavis Test Suite Architecture

This document describes the test architecture for Pavis, including the three test suites, unified test infrastructure, and authoring guidelines for shell-based test cases.

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

## Runtime Suite

### Components Involved

- `pavis` runtime binary (under test)
- `pavis-mock-upstream` (deterministic backend)
- `pavis-mock-relay` (minimal control plane for hot-reload)

### What Is Tested

The runtime suite validates the **Frozen Data Plane** guarantees:

- **Hot Reload**: The runtime can consume updated `.pvs` artifacts via long-poll without restarting.
- **Last-Known-Good (LKG)**: Invalid or corrupt artifacts are rejected, and the runtime continues using the last valid configuration.
- **Artifact Validation**: Malformed, version-mismatched, or corrupt `.pvs` files are detected and rejected at load time.
- **No-Restart / No-Drop Guarantees**: Configuration updates do not interrupt in-flight requests or require process restarts.

### Why Mock Relay?

Runtime hot-reload depends on long-poll artifact updates from a control plane. Using the real `pavis-relay` would introduce unnecessary dependencies (etcd, TLS, multi-node coordination) that are irrelevant to testing runtime semantics. The mock relay provides exactly the minimal interface needed: long-poll + publish.

### Emphasis

This suite is the **primary validation** of the Frozen Data Plane architecture. It ensures the runtime behaves correctly under dynamic configuration updates without coupling to the full control plane.

---

## Relay Suite

### Components Involved

- `pavis-relay` binary (under test)
- Mock or simulated runtime clients (if needed)
- NO `pavis-mock-upstream` (relay does not forward traffic)

### What Is Tested

The relay suite validates **control-plane semantics**:

- **Fanout**: Relay correctly distributes artifacts to multiple runtime subscribers.
- **Long-Poll Efficiency**: Relay uses ETags and conditional responses to minimize bandwidth.
- **Concurrency**: Relay handles simultaneous connections and publishes without race conditions.
- **Artifact Storage**: Relay persists and serves artifacts across restarts (if configured).

### What Is NOT Tested

- Traffic forwarding (relay does not proxy HTTP/TCP).
- Runtime hot-reload (that is the runtime suite's responsibility).

### Emphasis

This suite focuses exclusively on the relay's role as a control-plane artifact server. It does not involve the runtime or upstream services.

---

## Integrated Suite

### Components Involved

- Full end-to-end path: `pavctl` → `pavis-relay` → `pavis` runtime → `pavis-mock-upstream`

### What Is Tested

- **Smoke Tests**: Verify the complete system path works (pavctl compiles config → relay serves it → runtime consumes it → traffic flows).
- **Integration Proof**: Confirm that components integrate correctly (e.g., relay fanout reaches runtime, runtime hot-reloads, traffic succeeds).

### Design Constraint

- **Fewer Cases**: This suite is intentionally limited in scope. Full coverage is achieved by the runtime and relay suites.
- **Not a Replacement**: The integrated suite is a sanity check, not a comprehensive test bed.

### Emphasis

The integrated suite provides confidence that the pieces work together. It does not duplicate coverage from the other two suites.

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

3. **Kubernetes** (future):
   - Components run in a local or remote cluster.
   - Validates production-like orchestration.

### Key Principle

Environment differences affect **how components are launched**, not **what is tested**. The same test cases run in all environments with identical assertions.

---

## Design Rationale

### Why Mock Relay Exists

The `pavis` runtime's hot-reload mechanism requires a live control plane that implements long-poll artifact distribution. Testing runtime-only semantics (hot-reload, LKG, artifact validation) should not require the full complexity of the production relay (etcd, TLS, multi-node coordination). The mock relay provides the minimal interface needed: a long-poll endpoint and a publish endpoint for injecting test artifacts.

### Why Runtime-Only Tests Cannot Rely on Restart-Based Updates

The Frozen Data Plane architecture explicitly guarantees zero-downtime configuration updates. Testing hot-reload by restarting the runtime would violate the core design principle and fail to detect regressions in the long-poll mechanism.

### Why Integrated Tests Are Intentionally Limited

Full coverage of runtime behavior is achieved in the runtime suite. Full coverage of relay behavior is achieved in the relay suite. The integrated suite exists to prove the integration points work, not to re-test every feature in a slower, more complex environment.

---

## Case Authoring Guide

The following sections provide detailed guidance for writing individual test cases within the three suites.

---

## Scope & Philosophy

An E2E case in this repository is responsible for **verifying a specific behavior of the Pavis proxy** against a controlled upstream.

### Responsibilities

- **Configure Pavis**: Provide a valid configuration (file or inline).
- **Generate Traffic**: Send HTTP/TCP requests through Pavis.
- **Assert Behavior**: Verify the response from Pavis or the state of the upstream.

### Non-Responsibilities

- **Infrastructure**: Do NOT manage Docker containers, networks, or volumes. The runner handles this.
- **Lifecycle**: Do NOT start or stop the Pavis process manually unless testing crash recovery. The harness handles the primary process.
- **Complex Logic**: Keep scripts linear. Avoid complex loops or conditionals.

### Relationship Hierarchy

1. **Runner (`run.sh`)**: Orchestrates the suite, handles global setup/teardown (Docker).
2. **Suite**: A directory of related cases (e.g., `tests/suites/pavis`).
3. **Case**: A single shell script (e.g., `routing.sh`) that tests one feature.
4. **Upstream (`pavis-mock-upstream`)**: The deterministic backend service.

---

## Standard Case Structure

Every case script MUST follow this canonical structure:

```bash
#!/bin/bash
# Case: [Name]
# Scenario: [Brief description of what is being tested]

# 1. Imports
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

# 2. Configuration & Inputs
# Define variables or write config files relative to the case execution
cat > pavis.yaml <<EOF
...
EOF

# 3. Execution
# Start Pavis or apply config
pavis_start "pavis.yaml"

# 4. Traffic Generation
# Send requests with MANDATORY isolation headers
response=$(curl_pavis "/api/resource" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}" \
  -H "X-Pavis-Test-Run: ${RUN_ID}")

# 5. Assertions
# Validate HTTP status, headers, or upstream observations
assert_status 200 "$response"
assert_json_field "$response" "path" "/expected/path"

# 6. Cleanup (Optional)
# The runner handles process cleanup, but reset upstream state if necessary
pavis_upstream_reset
```

---

## Mandatory Test Isolation Headers

To ensure parallel safety and determinism, **EVERY** request sent through Pavis MUST include the following headers:

1. **`X-Pavis-Test-Run`**: Identifies the specific CI run or test session.
2. **`X-Pavis-Test-Case`**: Identifies the specific test case file/function.

### Why?

- **State Isolation**: The upstream uses these headers to namespace counters (e.g., for `/flaky` or `/received`).
- **Concurrency**: Allows multiple tests to hit the same upstream container simultaneously without cross-talk.
- **Debugging**: Traces logs clearly to a specific test case.

**Violation of this rule will cause flaky tests and CI failures.**

---

## Upstream Endpoint Usage Guide

The shared upstreams are powered by the workspace `pavis-mock-upstream` binary whenever `TEST_MODE=binary` (the default). The runner spawns two instances (backend-v1 on `8081/8443`, backend-v2 on `8082/8444`) from `$PAVIS_MOCK_UPSTREAM_BIN` using the TLS materials generated under `tests/config/certs`. When `TEST_MODE=docker`, each suite has its own `docker-compose.yaml` under `tests/suites/<suite>/` that spins up the same `pavis-testkit:local` image; run `make docker-build IMAGE=testkit MODE=local` beforehand so the image is available locally. The test harness automatically attaches the Pavis/Relay containers it launches (per test case) to that compose network so everything shares the same isolated virtual network.

The `pavis-mock-upstream` service is the source of truth for all backend behaviors. Use the correct endpoint for the feature you are testing.

| Feature | Endpoint | Purpose | Assertion Strategy |
| :--- | :--- | :--- | :--- |
| **Health** | `/healthz` | Container readiness | Assert `200 OK`. |
| **Routing/Metadata** | `/echo` | Verifying headers, paths, TLS | Assert JSON fields (`path`, `headers`, `tls.sni`). |
| **Error Handling** | `/status?code=N` | Testing Pavis error mapping | Assert Pavis passes through `N` (or maps it). |
| **Latency/Timeouts** | `/delay?ms=N` | Testing simple latency | Assert response time > `N` ms. |
| **Timeouts (Hard)** | `/hang?ms=N` | Testing timeouts | Assert Pavis returns `504` before `N` ms elapses. |
| **Retries** | `/flaky?fail=N` | Testing retry logic | Assert final `200 OK` (Pavis hid the errors). |
| **Circuit Breaking** | `/close` | Testing connection errors | Assert Pavis handles TCP RST gracefully (`503`). |
| **Payloads** | `/bytes?n=N` | Testing large bodies | Assert `Content-Length` matches `N`. |
| **Observability** | `/received` | Verifying request history | Assert request count or sequence. |
| **State Clearing** | `/reset` | Clearing counters | **Internal Use Only** (usually). |

---

## Feature-Oriented Patterns

### L7 Routing & Traffic Splitting

- **Endpoint**: `/echo`
- **Technique**: Send multiple requests.
- **Assertion**: Parse the `instance_id` field in the JSON response to verify which backend replica handled the request.
- **FAIL**: Relying on round-robin assumptions with low sample sizes.

### Header Manipulation & Rewrite

- **Endpoint**: `/echo`
- **Technique**: Send a request with (or without) specific headers/paths.
- **Assertion**: Verify the `headers` object or `path` field in the response JSON reflects the modifications made by Pavis.

### Retries

- **Endpoint**: `/flaky?fail=1` (or `N`)
- **Technique**: Configure Pavis to retry at least `N` times.
- **Assertion**:
  1. Client receives `200 OK`.
  2. (Optional) Query `/received` to verify upstream saw `N+1` attempts.

### Timeouts

- **Endpoint**: `/hang?ms=5000`
- **Technique**: Configure Pavis timeout to `1s`.
- **Assertion**: Client receives `504 Gateway Timeout` (generated by Pavis) roughly at 1s.
- **FAIL**: If the test hangs for 5s, the timeout failed.

### Upstream TLS (Origination)

- **Endpoint**: `/echo` (via HTTPS port)
- **Technique**: Configure Pavis to talk to upstream port `8443` (or equivalent).
- **Assertion**: Response JSON `tls.enabled` MUST be `true`. `tls.sni` MUST match configuration.

---

## Determinism Rules (STRICT)

1. **No Racing**: Never rely on "sleep 1" to wait for an async process. Poll a health endpoint or status file.
2. **Unordered JSON**: JSON keys are unordered. Use `jq` or specific helpers to extract fields, never `grep` across lines unless the format is guaranteed (pavis-mock-upstream guarantees sorted keys, but be careful).
3. **Explicit Ordering**: Do not assume request A arrives before request B unless you force it (synchronous execution).
4. **State Cleanup**: If you dirty the upstream state (e.g., `/flaky`), you MUST use `/reset` or unique headers to avoid polluting other tests.

---

## Forbidden Patterns (Hard Rules)

- **⛔ NO Docker commands**: `docker run`, `docker stop`, etc. are banned in case scripts.
- **⛔ NO Random Ports**: Do not bind to random ports. Use the assigned ports from the environment.
- **⛔ NO Client IP Assertions**: Do not assert `remote_addr` matches a specific IP. It changes in CI/Docker.
- **⛔ NO External Dependencies**: Do not `curl google.com`. All traffic must stay within the test network.
- **⛔ NO Log Grepping (mostly)**: Prefer asserting against metrics or upstream `/received` API. Log parsing is brittle and slow.

---

## Parallel-Safety & CI Readiness

- **Filesystem**: Write temporary config files to `${TMP_DIR}/${CASE_NAME}` (managed by runner), not global paths.
- **Upstream State**: Always rely on `X-Pavis-Test-Run` and `X-Pavis-Test-Case` headers. The upstream service uses these to segregate stateful counters.
- **Reset**: Calling `/reset` without namespace headers clears GLOBAL state. This is forbidden in parallel mode. Always scope your reset.

---

## Example Case Walkthrough

**Scenario**: Verify that Pavis retries a 503 error from the upstream.

1. **Setup**:
   - Target endpoint: `/flaky?fail=1`. This will return 503 once, then 200.
   - Pavis Config: `retry_policy: { attempts: 2, retry_on: [5xx] }`.
2. **Action**:
   - Test script generates ONE request: `curl http://pavis/flaky?fail=1`.
   - Headers: `X-Pavis-Test-Case: retry_demo`.
3. **Flow**:
   - Pavis -> Upstream (Attempt 1): Upstream returns 503. Counters increment.
   - Pavis sees 503, checks policy -> Matches.
   - Pavis -> Upstream (Attempt 2): Upstream returns 200.
4. **Assertion**:
   - Script receives `200 OK`.
   - Script queries `/received` with header `X-Pavis-Test-Case: retry_demo`.
   - Assert that `count` is 2.

---

## Checklist for New Cases

Before submitting a new test case, verify:

- [ ] **Headers**: Are `X-Pavis-Test-Run` and `X-Pavis-Test-Case` included in all requests?
- [ ] **Determinism**: Are you using a deterministic endpoint (`/echo`, `/status`)?
- [ ] **Assertions**: Are you asserting the specific JSON field or HTTP code required?
- [ ] **No Infra**: Did you remove any `docker` or `sleep` commands?
- [ ] **Cleanliness**: Is the test independent of previous test runs?
