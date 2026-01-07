# Audit Phase 5: Performance & Latency Signals
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. Hot Path Identification & Analysis

The critical path for every request involves `Proxy::request_filter` (routing) and `Proxy::upstream_peer` (forwarding).

### 1.1 `Router::match_request` (Routing Logic)
*   **Status**: **Highly Optimized**.
*   **Allocations**: Zero.
*   **Analysis**:
    *   Host normalization (`normalize_host`) operates on string slices (`&str`) without copying.
    *   `ExactMap` lookups use `&str` keys.
    *   Regex matching uses pre-compiled `Regex` instances.
    *   The `zones` iteration involves no heap allocations.

### 1.2 `Proxy::request_filter` (Request Processing)
*   **Status**: **Allocation Risk**.
*   **Signals**:
    *   **Header Policy Cloning**: `ctx.request_headers = route.request_headers.clone();`.
        *   *Impact*: Deep copies the entire header manipulation policy (vectors of strings) for **every** matched request. This is the significant performance bottleneck identified.
    *   **String Cloning**:
        *   `ctx.upstream_name = Some(dest.upstream.clone())`: Allocates a new string for the upstream name.
        *   `ctx.sni_override = Some(host.clone())`: Allocates a new string for SNI.
    *   **Rewrites**: Path rewriting naturally requires new string allocation (`String::with_capacity`), which is unavoidable but efficient.

### 1.3 `Proxy::upstream_peer` (Upstream Selection)
*   **Status**: **Minor Overhead**.
*   **Signals**:
    *   `cluster.select_endpoint()` returns a cloned `Endpoint`. If the endpoint uses `EndpointAddr::Ip`, this is relatively cheap, but `EndpointAddr::Dns` (future) would involve string cloning.

## 2. Performance Benchmarks Plan

To quantify these signals, the following benchmarks are proposed:

### 2.1 Routing Latency Suite
*   **Goal**: Measure pure routing overhead including the identified cloning costs.
*   **Scenarios**:
    1.  **Baseline**: Exact match, no headers, no rewrites. (Measures `match_request` + `UpstreamName` clone).
    2.  **Header Stress**: Route with 10+ request/response header manipulations. (Measures `HeadersPolicy` clone cost).
    3.  **Regex vs Prefix**: Compare regex matching cost vs prefix trie/linear scan.

### 2.2 Throughput under Reload
*   **Goal**: Verify atomic swap impact.
*   **Scenario**: Sustain 10k RPS while reloading config every 100ms. Measure p99 latency spikes.

### 2.3 Concurrency Scaling
*   **Goal**: Verify `AtomicUsize` and `Arc` scaling.
*   **Scenario**: 1 vs 4 vs 16 worker threads handling 100k RPS to the same round-robin cluster.

## 3. Findings Summary

The core routing logic (`Router`) is extremely efficient and allocation-free. However, the `Proxy` glue layer introduces per-request allocations (Cloning `HeadersPolicy`, `UpstreamName`, `Hostname`) that could be optimized (e.g., using `Arc` for policies or `Cow` for names) in future iterations.

**Verdict**: **Structural Issues (Optimizable)**. The design is sound, but the cloning of configuration data in the hot path is a clear performance signal.
