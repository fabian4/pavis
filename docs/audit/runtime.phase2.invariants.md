# Audit Phase 2: Runtime Invariants & Correctness
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. Invariant Inventory & Enforcement

The runtime architecture successfully enforces the following critical invariants:

| Invariant | Mechanism | Location | Verdict |
|-----------|-----------|----------|---------|
| **Config Immutability** | `RuntimeState` holds `Arc<Router>` and `Manager` which are effectively read-only after construction. | `src/state.rs`, `src/router.rs` | **Enforced** |
| **Atomic Hot Reload** | `RuntimeStateHandle` uses `arc_swap::ArcSwap` to atomically replace the entire state snapshot. | `src/state.rs` | **Enforced** |
| **Partial Update Prevention** | The state swap is all-or-nothing. A new `RuntimeState` is fully constructed before being swapped in. | `src/state.rs` | **Enforced** |
| **Deterministic Routing** | `Router` compilation explicitly prioritizes routes (Exact > Prefix/Regex) and preserves declaration order for same-priority rules. | `src/router.rs` | **Enforced** |
| **Safe Regex Usage** | Regexes are compiled strictly during initialization/config load, never during request handling. | `src/router.rs` | **Enforced** |
| **Load Balancing Atomicity** | `Cluster` uses `AtomicUsize` (aligned) for round-robin counters and `ArcSwap` for endpoint updates. | `src/upstream/cluster.rs` | **Enforced** |

## 2. Detailed Mechanism Analysis

### 2.1 Atomic Switch-over
The `RuntimeStateHandle` ensures that no request ever sees a partially applied configuration.
```rust
pub fn store(&self, state: RuntimeState) {
    self.inner.store(Arc::new(state));
}
```
Existing in-flight requests hold a reference (`Arc`) to the old `RuntimeState`. They continue to use the old router and upstream manager until they complete. New requests immediately acquire the new `RuntimeState`. This is the gold standard for lock-free reconfiguration.

### 2.2 Routing Determinism
The `Router` structures (Linear vs ExactMap zones) preserve the semantic intent of the configuration.
*   **Exact Map Optimization**: Consecutive Exact matches are grouped for O(1) lookup.
*   **Conflict Resolution**: If duplicate paths exist, the first one is inserted, and subsequent ones are ignored (standard "first match" wins), guaranteeing behavior matches the config file order.

### 2.3 Load Balancing State
Internal mutability for load balancing is handled safely:
*   `AlignedCounter` protects against false sharing on the hot path.
*   `Cluster` state (endpoints list) is updated atomically via `ArcSwap`, allowing background DNS resolution (future feature) to update endpoints without locking the request path.

## 3. Gaps & Observations

*   **RR Counter Reset**: Because `Manager` and `Cluster` are recreated entirely on config reload, load balancing counters (Round Robin) are reset to 0. This is an acceptable trade-off for the simplicity of the immutable snapshot model.
*   **Host Normalization**: `match_request` normalizes host headers (strips ports/brackets) before lookup. This is a positive invariant that ensures consistent matching regardless of client `Host` header formatting.

**Verdict**: **PASS**. The runtime rigorously enforces its invariants through the type system and atomic primitives.