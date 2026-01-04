# Test Plan: Pavis xDS Readiness

This document defines the comprehensive test strategy for validating the `pavis-core` and `pavis` runtime changes required for full xDS compatibility. It serves as the authoritative source for test case implementation.

## 1. Test Strategy Overview

The testing strategy strictly follows the layered architecture of Pavis. We validate behavior at the lowest possible layer before moving up.

*   **Layer 1: Pavis Core (Unit)**. Validates the structural integrity and constraints of the domain model.
*   **Layer 2: XDS Codec (Unit/Property)**. Validates the deterministic transformation of xDS snapshots into Pavis configuration.
*   **Layer 3: Pavis Runtime (Component)**. Validates the execution logic (DNS, Routing, Headers) in isolation using mocks where possible.
*   **Layer 4: Integration (E2E)**. Validates the full system behavior from config ingestion to traffic routing.

**Goal**: Ensure that valid xDS configurations result in predictable, safe, and correct traffic handling, and that invalid configurations are rejected early and explicitly.

---

## 2. Test Matrix

### A. pavis-core (Pure Domain Tests)

| ID | Test Case Name | Input Description | Expected Outcome | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **C-01** | `listener_name_required` | `Listener` struct with empty name string | Validation Error | Listener names are critical for selection. |
| **C-02** | `endpoint_address_variants` | Valid `Ip(SocketAddr)` and `Dns(String, u16)` variants | Struct creation succeeds | Ensure enum variants serialize/deserialize correctly. |
| **C-03** | `rewrite_policy_defaults` | `Route` without `rewrite` field | `rewrite` field is `None` | Backward compatibility for existing routes. |
| **C-04** | `header_action_defaults` | `HeaderAction` with old schema (if supported) | Maps to `HeaderActionType::Set` | Ensure migration safety if manual conversion isn't forced. |
| **C-05** | `validate_listeners_non_empty` | `RuntimeConfig` with empty `listeners` vector | Validation Error | A config with no listeners is non-functional. |
| **C-06** | `validate_dns_endpoint_port` | `EndpointAddress::Dns` with port 0 | Validation Error | DNS endpoints must have explicit ports. |

### B. pavis-codec-xds (Transformation Tests)

| ID | Test Case Name | Input Description | Expected Outcome | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **X-01** | `map_lds_to_listeners` | xDS Snapshot with 3 Listeners | `RuntimeConfig.listeners` has 3 entries | 1:1 mapping, no filtering at codec level. |
| **X-02** | `reject_complex_listener` | Listener with multiple filter chains | `CodecError::UnsupportedFeature` | We only support flattened listeners. |
| **X-03** | `map_logical_dns_cluster` | Cluster type `LOGICAL_DNS` | `Upstream.discovery` = `LogicalDns` | Correct enum mapping. |
| **X-04** | `map_strict_dns_cluster` | Cluster type `STRICT_DNS` | `Upstream.discovery` = `StrictDns` | Correct enum mapping. |
| **X-05** | `map_prefix_rewrite` | Route with `prefix_rewrite` | `Route.rewrite.path_prefix_rewrite` set | Correct field mapping. |
| **X-06** | `map_host_rewrite` | Route with `host_rewrite_literal` | `Route.rewrite.host_rewrite_literal` set | Correct field mapping. |
| **X-07** | `map_header_append` | `request_headers_to_add` with `append: true` | `HeaderActionType::Append` | Correct enum mapping. |
| **X-08** | `deterministic_output` | Two identical xDS snapshots | Identical `pavis-core` bytes | Transformation must be deterministic. |
| **X-09** | `default_timeouts` | Route without timeouts | `timeout_ms` populated with default | Codec must apply defaults. |
| **X-10** | `default_retry_policy` | Route with partial retry config | Full `RetryPolicy` populated | Codec ensures completeness. |

### C. pavis runtime (Behavioral Tests)

| ID | Test Case Name | Input Description | Expected Outcome | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **R-01** | `boot_multiple_listeners` | Config with 2 listeners | Both ports bound and accepting traffic | Multi-listener support. |
| **R-05** | `header_append_comma` | Existing `User-Agent` lines + `Append` | Collapsed into single `, `-joined line | Joinable headers are merged. |
| **R-06** | `header_append_cookie` | Existing `Set-Cookie` + `Append` (mixed case) | Multiple header lines emitted | Non-joinable, case-insensitive. |
| **R-07** | `rewrite_prefix_root` | Route `/api` -> `/v1`, Request `/api/foo` | Path becomes `/v1/foo` | Standard prefix replacement. |
| **R-08** | `rewrite_prefix_exact` | Route `/api` -> `/v1`, Request `/api` | Path becomes `/v1` | Exact match replacement. |
| **R-09** | `rewrite_host_header` | `host_rewrite_literal` = "backend" | `Host` header sent upstream is "backend" | Host modification check. |

### D. DNS & LKG-Specific Tests

| ID | Test Case Name | Input Description | Expected Outcome | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **D-01** | `dns_resolve_initial` | `StrictDns` upstream, hostname resolves to 2 IPs | LB has 2 healthy endpoints | Basic resolution success. |
| **D-02** | `dns_resolve_empty` | Hostname resolves to 0 IPs | LB retains LKG endpoints, logs warning | Empty set must not clobber. |
| **D-03** | `dns_failure_lkg` | Initial success (2 IPs) -> DNS failure | LB retains previous 2 IPs | LKG safety net. |
| **D-04** | `dns_recovery` | Initial success -> Failure (LKG) -> Success (new IPs) | LB updates to new IPs | Recovery from transient failure. |
| **D-05** | `logical_dns_cardinality` | `LogicalDns` upstream, hostname resolves to 5 IPs | LB uses exactly 1 IP (best effort) | Distinction from StrictDns. |
| **D-06** | `strict_dns_cardinality` | `StrictDns` upstream, hostname resolves to 5 IPs | LB uses all 5 IPs | Distinction from LogicalDns. |
| **D-07** | `dns_hot_swap` | Resolution update while requests in flight | Requests complete, new reqs use new IPs | Atomic state swap check. |

### E. Integration / End-to-End Scenarios

| ID | Test Case Name | Input Description | Expected Outcome | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **I-01** | `e2e_minimal_routing` | Simple xDS with 1 listener, 1 route, static IP | Traffic flows to backend | Baseline sanity check. |
| **I-02** | `e2e_dns_upstream` | Config with `StrictDns` pointing to `localhost` | Traffic flows to local backend | Validates resolver integration. |
| **I-03** | `e2e_rewrite_and_append` | Route with prefix rewrite AND header append | Backend sees rewritten path + appended header | Feature interaction test. |
| **I-04** | `e2e_runtime_reload_dns` | Hot-reload config while DNS resolver active | Old resolver stops, new one starts | Resource cleanup check. |
| **I-05** | `e2e_weighted_dns` | Weighted split between Static IP and DNS upstream | Traffic distributed per weight | LB integration test. |

---

## 3. Critical Invariants

The following invariants MUST hold true across all tests. Any violation is a critical regression.

1.  **Rewrite Ordering**: Rewrite actions MUST ONLY apply to the request *after* a route match has effectively "locked in". The routing decision is based on the *original* request path/host.
2.  **LKG Safety**: A running proxy MUST NEVER discard a valid upstream endpoint set for an empty/failed set due to transient DNS errors.
3.  **Listener Multiplicity**: The runtime MUST attempt to start every configured listener. Any bind failure must surface as an error.
4.  **No Silent Failure**: If a configured listener cannot be found, or a DNS name is malformed, the runtime MUST log an error and/or fail startup.

---

## 4. Failure-Oriented Tests (Negative Cases)

*   **F-01: Empty Listener List**. Provide a config with `listeners: []`. Expect immediate startup failure.
*   **F-02: Duplicate Listener Ports**. Provide a config with 2 listeners binding to the same port. Expect bind failure.
*   **F-03: Malformed DNS**. Provide an upstream with `address: Dns("invalid...host", 80)`. Expect runtime error logs + empty endpoint set (safe failure).
*   **F-04: Rewrite Root to Empty**. Prefix rewrite `/` to `` (empty string). Expect valid behavior (likely `/`) or defined error, not panic.
*   **F-05: Header Remove Non-Existent**. Action `Remove` on a missing header. Expect no-op, no error.

---

## 5. Observability Expectations

Tests should assert that the following observability signals are emitted:

*   **Logs**:
    *   `INFO`: "Listener registered" with `{name}` at startup.
    *   `INFO`: "DNS resolution updated upstream" with `{upstream}` and `{count}`.
    *   `WARN`: "DNS resolution failed" with `{upstream}` and LKG retained.
    *   `ERROR`: Listener bind failures surface to startup error.
*   **Metrics** (if available):
    *   `pavis_upstream_dns_resolve_total`: Counter.
    *   `pavis_upstream_dns_resolve_failures`: Counter.
    *   `pavis_upstream_active_endpoints`: Gauge.

---

## 6. Test Update Checklist

This checklist tracks the full scope of unit, integration, and e2e tests for xDS readiness.

### A. Unit Tests

*   **LDS mapping**
    *   Map listener name + address.
    *   Reject multiple addresses.
    *   Reject multiple filter chains.
    *   Reject SNI-based matching.
*   **CDS mapping**
    *   Map `STATIC` -> `DiscoveryType::Static` (IP literals only).
    *   Map `LOGICAL_DNS` -> `DiscoveryType::LogicalDns` (hostname only).
    *   Map `STRICT_DNS` -> `DiscoveryType::StrictDns` (hostname only).
    *   Reject EDS clusters.
    *   Map supported load balancer values.
    *   Map HTTP protocol options + defaults.
*   **RDS mapping**
    *   Prefix/Exact/Regex match mapping + regex length limits.
    *   Preserve route ordering.
    *   Header add/remove mapping (append vs set, remove).
    *   Rewrite mapping (prefix + host).
    *   Weighted destinations mapping + upstream existence checks.
    *   Timeout + retry policy defaults.
*   **Core validation boundary**
    *   Non-empty listeners.
    *   DNS endpoint port non-zero.
    *   Duplicate upstream/route detection.
    *   Header name/value constraints.
*   **Determinism**
    *   Identical snapshot input yields identical serialized output.

### B. Integration Tests

*   Codec output loads through runtime validation without mutation.
*   Deterministic output remains stable through serialize/deserialize cycle.
*   Runtime accepts codec output across LDS/CDS/RDS combinations.
*   Error propagation includes resource type, name, and field path.

### C. E2E Tests

*   Minimum routing path: xDS -> codec -> runtime -> backend traffic.
*   DNS upstream traffic with `StrictDns`.
*   Rewrite + header append interaction.
*   Runtime reload with active DNS resolver.
*   Weighted split between static and DNS upstreams.
