# Pavis Engineering Thesis Roadmap

## 1. Positioning
This roadmap records semantic closure for an engineering thesis. It is not a feature funnel, adoption plan, or product backlog. The sole objective is to prove that the Frozen Data Plane model works end-to-end: compile every semantic upfront, seal it into `.pvs`, and run it with a dumb executor.

## 2. Closed Phases
### Compiler Pipeline
**Goal:** all semantics pass through typed compilation stages so the runtime never interprets config. **Status:** closed. `Artifact → RuntimeConfig → ValidatedRuntimeConfig → .pvs` is implemented, enforced by Phase 3.5 guards, and covered by relay+proxy integration tests.

### Artifact Model
**Goal:** `.pvs` is the only contract between compiler, relay, and runtime. **Status:** closed for structure and tooling. Magic bytes, checksums, corruption rejection, and `pavctl` generation/inspection flows are in place; relay handles artifacts opaquely.

### Execution & Reload Semantics
**Goal:** fail-closed, atomic reload with deterministic recovery. **Status:** closed. File ingest, ETag distribution, ArcSwap reloads, LKG persistence, graceful shutdown, admin API, lineage tracking, and the modular runtime architecture (isolated bootstrap, phase-typed contexts, and pre-resolved DNS) are implemented and exercised.

### Security & Identity
**Goal:** freeze TLS, mTLS, RBAC, and SPIFFE semantics. **Status:** closed. OpenSSL-only runtime, client-auth enforcement, outbound CA bundles, deny-by-default RBAC, and workload identity extraction ship as compiled artifacts with TLS E2E coverage.

### Observability & Lifecycle
**Goal:** provide metrics/logs/traces without runtime inference. **Status:** closed. Prometheus metrics, structured access logs, OTLP tracing, runtime stats, and lifecycle controls (graceful drain, admin health) operate entirely off frozen config.

### Relay Boundary
**Goal:** keep relay dumb and enforce monotonic versioning. **Status:** closed. Relay treats artifacts as opaque blobs, manages LKG promotion, and satisfies the reject/accept protocol relied on by the runtime.

## 3. Terminal Closure Gates
- **`.pvs` versioning contract** — compatibility rules for artifact evolution remain unwritten. Thesis completion requires freezing this contract so all builders agree on ABI guarantees.
- **Release discipline** — deterministic CI/CD across linux/amd64 and linux/arm64 (plus Cargo publishing) must be operational to prove sealed artifacts are identical regardless of builder. Until then, the execution pipeline cannot be declared permanently closed.

## 4. Optional / Exploratory Work
- **xDS ingest adapter (🧊 design-only)** — would compile ADS snapshots into `.pvs` without ever teaching the runtime to speak xDS. Deferred until the artifact ABI is frozen.
- **Kubernetes ingest/operator (🧊 design-only)** — would translate CRDs into artifacts while leaving the runtime unchanged. Deferred for the same reason.
- **Crash-consistency failpoints (🧊 design-only)** — deterministic crash injection for relay publish/runtime apply. Optional hardening once persistence layouts stop moving.
These explorations do not block semantic closure and carry no delivery commitment.

## 5. Rejected After Implementation
- Runtime DSLs (WASM/Lua), inline scripting, and gateway plug-ins were removed because they violate the Frozen Data Plane invariant.
- Runtime xDS, service-mesh behaviors, and control-plane integration inside the executor were rejected; only compiled artifacts may drive behavior.
- Graceful degradation, best-effort recovery, and heuristic rollback were intentionally excluded after proving that fail-closed semantics keep state auditable and deterministic.