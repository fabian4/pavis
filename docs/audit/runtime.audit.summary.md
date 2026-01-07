# Audit Summary: `crates/pavis` (Runtime)
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. Executive Verdict
**Verdict**: **Mostly Sound (Needs Fixes)**

The `pavis` runtime is architecturally robust, exhibiting high standards of safety, error discipline, and concurrency correctness. It adheres strictly to the "Immutable Snapshot" pattern, ensuring safe hot reloads and lock-free request processing.

However, a **Performance Optimization Defect** was identified in the hot path: the runtime deep-clones header manipulation policies for every request. While functionally correct, this violates the efficiency requirement for a high-performance data plane and should be remediated before large-scale production use with complex configurations.

## 2. Top System Risks

1.  **Hot Path Allocation (Performance Risk)**
    *   *Origin*: Phase 5 (Performance)
    *   *Detail*: `Proxy::request_filter` performs a deep clone of `HeadersPolicy` (vectors of strings) for every matched request.
    *   *Impact*: Significant allocator pressure and throughput degradation for routes with extensive header modification rules.

2.  **Load Balancing Reset (Traffic Risk)**
    *   *Origin*: Phase 2 (Invariants)
    *   *Detail*: The "Shared-Nothing" snapshot architecture implies that `UpstreamManager` and `Cluster` states are recreated on every reload.
    *   *Impact*: Round-Robin counters are reset to zero. Frequent reloads could cause transient load skew towards the first endpoints in the list.

3.  **SNI Defaulting (Observability Risk)**
    *   *Origin*: Phase 1 (Boundary)
    *   *Detail*: If TLS is enabled but no SNI is configured/overridden, the runtime defaults SNI to `"localhost"`.
    *   *Impact*: While preventing crashes, this might mask configuration errors where the upstream expects a specific SNI, leading to confusing 421 or 404 errors from the upstream.

## 3. Readiness Assessment

| Category | Assessment | Notes |
|----------|------------|-------|
| **Boundary Purity** | **YES** | Strictly consumes validated `.pvs` artifacts. No raw parsing. |
| **Runtime Invariants** | **YES** | `ArcSwap` and `AtomicUsize` ensure correct lock-free operation. |
| **Error Diagnostics** | **YES** | Zero panics. Structured logs with high-context fields. |
| **Concurrency Safety** | **YES** | Thread-safe architecture. No `unsafe` blocks. |
| **Performance** | **NO** | Allocation in hot path requires optimization. |

## 4. Next Steps

### Critical (Must Fix)
1.  **Optimize Header Policy Storage**: Refactor `pavis_core::Route` or the runtime `Proxy` logic to wrap `HeadersPolicy` in `Arc<T>` or use `Cow<'a, T>`. This will eliminate the deep clone in `request_filter` and make header manipulation allocation-free.
2.  **Execute Benchmarks**: Run the "Header Stress" benchmark proposed in Phase 5 to quantify the current overhead and verify the fix.

### Recommended
3.  **Refine SNI Logic**: Consider logging a warning when the "localhost" SNI default is applied, or strictly requiring SNI configuration for TLS upstreams.
4.  **LB State Persistence**: Investigate strategies to preserve load balancing counters (e.g., via a shared state registry) across configuration reloads to smooth out traffic distribution during frequent updates.
