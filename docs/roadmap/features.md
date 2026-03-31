# Capability Index

> **Authority:** Feature status follows `docs/roadmap/roadmap.md`. This file is a factual index, not a wishlist.

## Legend
- ✅ **Closed** — Implemented and verified under the Frozen Data Plane rules.
- ⚠️ **Partial** — Implemented with a known closure gap tracked in the roadmap.
- 🧊 **Optional** — Design-only or deferred work; not required for semantic closure.
- ❌ **Rejected** — Explicitly out of scope.

## Traffic Management
- ✅ **L7 routing** — Deterministic prefix/exact/regex routing trees are compiled into the artifact; constraint: no OR/NOT predicates or runtime evaluation.
- ✅ **Traffic splitting** — Weighted round-robin across upstream clusters is frozen in the artifact; constraint: only static weights.
- ✅ **Header manipulation** — Insert/remove/overwrite policies are materialized ahead of time; constraint: no runtime templating.
- ✅ **Redirects & direct responses** — Fixed HTTP responses and 3xx rewrites are encoded at compile time; constraint: no dynamic body generation.
- ✅ **Host/path rewrite** — Literal host overrides and prefix path rewrites are supported; constraint: no regex substitution.
- ✅ **Pre-resolved DNS** — Upstream endpoints are resolved during artifact load, eliminating per-request DNS lookups; constraint: only Strict/Logical discovery modes supported.
- ✅ **Retries & timeouts** — Fixed retry budgets, error classes, per-try/backoff settings, and buffering rules are enforced; constraint: idempotency requirements must be encoded by the codec.
- 🧊 **Traffic mirroring** — Shadow copies are intentionally absent; constraint: design-only until a compiler proof exists.
- 🧊 **Local rate limiting** — No token-bucket or adaptive throttles are implemented; constraint: would require runtime state.
- ❌ **Global rate limiting** — Rejected to avoid external runtime dependencies.

## Security & Identity
- ✅ **Server TLS termination** — Single-cert listeners via OpenSSL backend; constraint: one workload identity per listener.
- ✅ **Upstream TLS origination** — Per-upstream CA bundles, SNI overrides, and TLS verify modes are frozen; constraint: runtime cannot change trust anchors.
- ✅ **Inbound mTLS** — Client certificate enforcement (optional/required) is compiled; constraint: no on-the-fly CA rotation.
- ✅ **Outbound mTLS** — Client cert/key/chain selection is materialized; constraint: secrets must exist on disk before load.
- ✅ **RBAC** — Deny-by-default method/path policies are sealed into the artifact; constraint: no attribute-based or dynamic auth.
- ✅ **SPIFFE identity extraction** — SPIFFE IDs are surfaced from the TLS stack; constraint: no runtime rewriting.
- ❌ **SNI multi-cert** — Rejected; multiple identities belong at ingress.
- ❌ **External auth / OIDC** — Rejected; frozen data plane does not call out to auth services.
- ❌ **WAF / deep inspection** — Rejected; cost and policy ambiguity violate the thesis.

## Resilience
- ✅ **Active health checks** — Periodic GET probes (with TLS policy) are defined in config; constraint: only 2xx is healthy.
- ✅ **Passive outlier detection** — Failure budget thresholds eject endpoints; constraint: counters reset on reload.
- ✅ **Circuit breaking** — Connection/pending limits enforced via semaphores; constraint: rejection is immediate 503.
- ✅ **Connection pools** — Pool reuse keys honor frozen TLS/SNI policy; constraint: no adaptive pooling logic.
- 🧊 **Fault injection** — Chaos hooks are not implemented; constraint: would require runtime toggles.

## Observability
- ✅ **Prometheus metrics** — Request/upstream metrics with bounded cardinality come from frozen labels; constraint: no ad-hoc dimensions.
- ✅ **Access logs** — Structured JSON logs with request IDs and timings; constraint: log fields are fixed at compile time.
- ✅ **Distributed tracing (OTLP)** — Span emission adheres to frozen sampling + endpoint config; constraint: no runtime sampling changes.
- ✅ **Runtime stats / admin API** — `/health` and `/stats` expose read-only counters; constraint: interface is unauthenticated and must be network-restricted.
- ❌ **Tap / packet capture** — Rejected; use system tooling (tcpdump/eBPF).

## Operational Lifecycle
- ⚠️ **Hot reload** — Atomic pointer swaps via `ArcSwap` keep traffic flowing for `RuntimeState`-safe fields; constraint: changes to listeners/admin/shutdown/metrics/access-log are rejected until boot-time service graph reload exists.
- ✅ **Modularized bootstrap** — Startup and reload phases are isolated from core proxy logic; constraint: IO-layer is fully decoupled from config materialization.
- ✅ **Graceful shutdown** — Draining behavior is encoded through the shutdown policy; constraint: no per-request overrides.
- ✅ **LKG fallback** — Last-known-good artifacts remain live on rejection; constraint: manual rollback requires resealing a prior artifact.
- ⚠️ **`.pvs` versioning policy** — Format exists but compatibility rules are not frozen; constraint: release is blocked until documented.
- ⚠️ **Release discipline** — Multi-arch CI/CD and Cargo publishing are not yet automated; constraint: builders cannot claim deterministic parity.

## Compiler & Artifact Pipeline
- ✅ **Typed compiler stages** — Source intent passes through codec, core validation, and sealing; constraint: no bypass around validation.
- ✅ **Artifact integrity tooling** — `pavctl` gen/check/view flows plus relay checksum enforcement are complete; constraint: runtime refuses any artifact that fails integrity.
- 🧊 **Crash-consistency failpoints** — Optional hardening for relay publish/runtime apply is deferred; constraint: no failpoint harness today.
- 🧊 **xDS ingest adapter** — Design-only ADS capture to `.pvs`; constraint: runtime will never speak xDS.
- 🧊 **Kubernetes ingest/operator** — Design-only CRD compiler; constraint: runtime continues to consume sealed artifacts only.

## Rejected Scope
- ❌ **Runtime DSLs (WASM/Lua)** — Rejected to preserve the Frozen Data Plane invariant.
- ❌ **Gateway/platform features** — No gRPC transcoding, no external auth, no ingress-style policy surface.
- ❌ **Heuristic recovery** — No graceful degradation, best-effort merges, or auto-rollbacks; failures stay closed until a new artifact is sealed.
