# Pavis E2E Test Authoring Guide

> Source of truth for shell-based suites under `tests/`. This file mirrors the guidance previously in `docs/testing/case-authoring.md`.

## 1. Scope & Philosophy

An E2E case in this repository is responsible for **verifying a specific behavior of the Pavis proxy** against a controlled upstream.

### Responsibilities
* **Configure Pavis:** Provide a valid configuration (file or inline).
* **Generate Traffic:** Send HTTP/TCP requests through Pavis.
* **Assert Behavior:** Verify the response from Pavis or the state of the upstream.

### Non-Responsibilities
* **Infrastructure:** Do NOT manage Docker containers, networks, or volumes. The runner handles this.
* **Lifecycle:** Do NOT start or stop the Pavis process manually unless testing crash recovery. The harness handles the primary process.
* **Complex Logic:** Keep scripts linear. Avoid complex loops or conditionals.

### Relationship Hierarchy
1. **Runner (`run.sh`):** Orchestrates the suite, handles global setup/teardown (Docker).
2. **Suite:** A directory of related cases (e.g., `tests/suites/pavis`).
3. **Case:** A single shell script (e.g., `routing.sh`) that tests one feature.
4. **Upstream (`pavis-upstream`):** The deterministic backend service.

---

## 2. Standard Case Structure (Shell-Oriented)

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

## 3. Mandatory Test Isolation Headers

To ensure parallel safety and determinism, **EVERY** request sent through Pavis MUST include the following headers:

1. **`X-Pavis-Test-Run`**: Identifies the specific CI run or test session.
2. **`X-Pavis-Test-Case`**: Identifies the specific test case file/function.

### Why?
* **State Isolation:** The upstream uses these headers to namespace counters (e.g., for `/flaky` or `/received`).
* **Concurrency:** Allows multiple tests to hit the same upstream container simultaneously without cross-talk.
* **Debugging:** Traces logs clearly to a specific test case.

**Violation of this rule will cause flaky tests and CI failures.**

---

## 4. Upstream Endpoint Usage Guide

The shared upstreams are powered by the workspace `pavis-upstream` binary whenever `TEST_MODE=binary` (the default). The runner spawns two instances (backend-v1 on `8081/8443`, backend-v2 on `8082/8444`) from `$PAVIS_UPSTREAM_BIN` using the TLS materials generated under `tests/config/certs`. When `TEST_MODE=docker`, each suite has its own `docker-compose.yaml` under `tests/suites/<suite>/` that spins up the same `pavis-upstream:local` image; run `make docker-build IMAGE=upstream MODE=local` beforehand so the image is available locally. The test harness automatically attaches the Pavis/Relay containers it launches (per test case) to that compose network so everything shares the same isolated virtual network.

The `pavis-upstream` service is the source of truth for all backend behaviors. Use the correct endpoint for the feature you are testing.

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

## 5. Feature-Oriented Patterns

### L7 Routing & Traffic Splitting
* **Endpoint:** `/echo`
* **Technique:** Send multiple requests.
* **Assertion:** Parse the `instance_id` field in the JSON response to verify which backend replica handled the request.
* **FAIL:** Relying on round-robin assumptions with low sample sizes.

### Header Manipulation & Rewrite
* **Endpoint:** `/echo`
* **Technique:** Send a request with (or without) specific headers/paths.
* **Assertion:** Verify the `headers` object or `path` field in the response JSON reflects the modifications made by Pavis.

### Retries
* **Endpoint:** `/flaky?fail=1` (or `N`)
* **Technique:** Configure Pavis to retry at least `N` times.
* **Assertion:**
  1. Client receives `200 OK`.
  2. (Optional) Query `/received` to verify upstream saw `N+1` attempts.

### Timeouts
* **Endpoint:** `/hang?ms=5000`
* **Technique:** Configure Pavis timeout to `1s`.
* **Assertion:** Client receives `504 Gateway Timeout` (generated by Pavis) roughly at 1s.
* **FAIL:** If the test hangs for 5s, the timeout failed.

### Upstream TLS (Origination)
* **Endpoint:** `/echo` (via HTTPS port)
* **Technique:** Configure Pavis to talk to upstream port `8443` (or equivalent).
* **Assertion:** Response JSON `tls.enabled` MUST be `true`. `tls.sni` MUST match configuration.

---

## 6. Determinism Rules (STRICT)

1. **No Racing:** Never rely on "sleep 1" to wait for an async process. Poll a health endpoint or status file.
2. **Unordered JSON:** JSON keys are unordered. Use `jq` or specific helpers to extract fields, never `grep` across lines unless the format is guaranteed (pavis-upstream guarantees sorted keys, but be careful).
3. **Explicit Ordering:** Do not assume request A arrives before request B unless you force it (synchronous execution).
4. **State Cleanup:** If you dirty the upstream state (e.g., `/flaky`), you MUST use `/reset` or unique headers to avoid polluting other tests.

---

## 7. Forbidden Patterns (Hard Rules)

* **⛔ NO Docker commands:** `docker run`, `docker stop`, etc. are banned in case scripts.
* **⛔ NO Random Ports:** Do not bind to random ports. Use the assigned ports from the environment.
* **⛔ NO Client IP Assertions:** Do not assert `remote_addr` matches a specific IP. It changes in CI/Docker.
* **⛔ NO External Dependencies:** Do not `curl google.com`. All traffic must stay within the test network.
* **⛔ NO Log Grepping (mostly):** Prefer asserting against metrics or upstream `/received` API. Log parsing is brittle and slow.

---

## 8. Parallel-Safety & CI Readiness

* **Filesystem:** Write temporary config files to `${TMP_DIR}/${CASE_NAME}` (managed by runner), not global paths.
* **Upstream State:** Always rely on `X-Pavis-Test-Run` and `X-Pavis-Test-Case` headers. The upstream service uses these to segregate stateful counters.
* **Reset:** Calling `/reset` without namespace headers clears GLOBAL state. This is forbidden in parallel mode. Always scope your reset.

---

## 9. Example Case Walkthrough (Conceptual)

**Scenario:** Verify that Pavis retries a 503 error from the upstream.

1. **Setup:**
   * Target endpoint: `/flaky?fail=1`. This will return 503 once, then 200.
   * Pavis Config: `retry_policy: { attempts: 2, retry_on: [5xx] }`.
2. **Action:**
   * Test script generates ONE request: `curl http://pavis/flaky?fail=1`.
   * Headers: `X-Pavis-Test-Case: retry_demo`.
3. **Flow:**
   * Pavis -> Upstream (Attempt 1): Upstream returns 503. Counters increment.
   * Pavis sees 503, checks policy -> Matches.
   * Pavis -> Upstream (Attempt 2): Upstream returns 200.
4. **Assertion:**
   * Script receives `200 OK`.
   * Script queries `/received` with header `X-Pavis-Test-Case: retry_demo`.
   * Assert that `count` is 2.

---

## 10. Checklist for New Cases

Before submitting a new test case, verify:

- [ ] **Headers:** Are `X-Pavis-Test-Run` and `X-Pavis-Test-Case` included in all requests?
- [ ] **Determinism:** Are you using a deterministic endpoint (`/echo`, `/status`)?
- [ ] **Assertions:** Are you asserting the specific JSON field or HTTP code required?
- [ ] **No Infra:** Did you remove any `docker` or `sleep` commands?
- [ ] **Cleanliness:** Is the test independent of previous test runs?
