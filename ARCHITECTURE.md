# Architecture: Frozen Data Plane Constitution

## 1. Axioms / Invariants
- **Frozen Data Plane** — Every semantic decision (routing, retries, TLS, RBAC, health, observability) **MUST** be resolved before serialization. No runtime component may construct or reinterpret policy.
- **No Runtime Interpretation** — The runtime **MUST NOT** parse text configs, infer defaults, evaluate scripts, or execute dynamic code. It only deserializes trusted `.pvs` artifacts produced by the compiler pipeline.
- **Atomic Reload Only** — Configuration transitions **MUST** occur via an all-or-nothing swap of the entire artifact. Partial updates, incremental edits, and mutable in-place structures are forbidden.
- **Fail-Closed Semantics** — Any validation error, environment violation, or artifact incompatibility **MUST** leave the runtime serving the last-known-good state. There is no graceful degradation, no fallback heuristics, and no best-effort mode.

## 2. Semantic Boundary
- **Compile-Time Decisions (Mandatory)**
  - Routing graphs, matcher predicates, header policies, rewrite plans, retry budgets, timeout values, circuit-breaker thresholds, health-check descriptions, TLS and mTLS settings, RBAC rules, telemetry field sets, upstream discovery modes, and listener layouts **MUST** be baked into `RuntimeConfig` before artifact sealing.
  - Any ambiguity, missing field, or optional behavior **MUST** be rejected by the codec. `Option<T>` in runtime types is reserved for explicit “enabled/disabled” states, never for “use default.”
- **Runtime Validation (Allowed Scope)**
  - The runtime may perform environment checks: file readability, key/cert presence, socket binding, DNS resolution reachability, OpenSSL initialization, and OS resource availability.
  - Runtime validation **MUST NOT** change semantics. If the environment check fails, the entire artifact is rejected and the previous artifact remains live.

## 3. Artifact Contract (`.pvs`)
- `.pvs` is a versioned binary ABI shared between codec, relay, and runtime. Layout changes **MUST** follow the roadmap gate for the versioning policy; until that contract is frozen, artifacts are considered unstable.
- Every artifact carries magic bytes, version metadata, and checksums. If any of these fail to match expectations, the runtime **MUST** abort the load.
- Corruption, mismatched architecture, or unsupported version **MUST** cause immediate rejection and a return to LKG. The relay is responsible for monotonic versioning; the runtime never mutates artifacts and never attempts repair.

## 4. Failure Semantics
- Reload success is binary. Either the artifact validates and atomically replaces the live state, or it is rejected with no partial side effects.
- Rollback is not a special path; refusal to load a new artifact simply keeps the current state. Manual rollback requires resealing or redelivering a prior artifact.
- LKG is authoritative. The runtime **MUST** keep serving the last applied artifact until a new artifact is proven valid. There is no shadow config, preview mode, or best-effort merging.
- There is no graceful degradation. If upstreams fail or artifacts are invalid, the runtime fails-closed and surfaces explicit errors instead of improvising behavior.

## 5. Relay Distribution Loop (Runtime)
- The runtime fetch loop is modeled as a pure FSM plus a driver that executes effects; no I/O occurs inside the FSM.
- Fetches are strictly single in-flight. The runtime long-polls with a fixed `wait_ms` and immediately re-issues the next poll on NoUpdate.
- `204`/`304` are NoUpdate and never trigger backoff. `410` is NeedResync and forces an unconditional refetch without backoff.
- `5xx` and network failures are transient and trigger capped exponential backoff.
- Deduplication is checksum-based: artifacts with the same ETag are never re-applied.
