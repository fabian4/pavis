# Pavis Feature Matrix & Envoy Comparison

> **Authority:** This document is a derived index. All authoritative status and roadmap decisions are defined in `roadmap.md`.

## Introduction

Pavis is designed as a **Pragmatic & Lightweight Sidecar** for microservices. It is NOT a clone of Envoy and does not aim to support every feature of a general-purpose edge gateway.

This document provides a feature status overview for reference.

**Legend:**
*   ✅ **Supported**: Implementation is complete and verified.
*   🚧 **Partial**: Feature is present but has known correctness gaps (see ROADMAP.md).
*   ⏳ **Planned**: Currently on the critical path for upcoming releases.
*   ⚠️ **Deferred**: Recognized as valuable but prioritized below critical path items.
*   ❌ **Dropped**: Explicitly out of scope; effectively "WontFix" by design.

---

## 1. Traffic Management

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **L7 Routing** (Path/Header/Method) | ✅ | Advanced routing with multi-method predicates (`methods: ["GET", "POST"]`), header operators (exact/prefix/regex/present/absent), compound AND logic for multiple headers. OR/NOT predicates deferred. |
| **Traffic Splitting** (Canary) | ✅ | Weighted round-robin supported. |
| **Header Manipulation** | ✅ | Add/Remove headers supported. |
| **Redirect & DirectResponse** | ✅ | Supported. For HTTP->HTTPS or security blocking. |
| **Rewrite** (Host/Path) | ✅ | Prefix & Host literal supported. (No Regex rewrite). |
| **Retries & Timeouts** | ✅ | Full P2 retry policy implemented: backoff strategies (fixed, linear, exponential), retryable reasons filtering (status_code, connect_timeout, read_timeout, per_try_timeout, pool_full, connect_error), idempotency constraints, request body buffering for replay. Verified with E2E tests covering success, exhaustion, budget enforcement, and replayability. |
| **Traffic Mirroring** (Shadowing) | ⚠️ | Deferred. Not critical for MVP. |
| **Global Rate Limiting** | ❌ | **Dropped**. Too heavy (requires ext Redis/gRPC). Use Ingress. |
| **Local Rate Limiting** | ⚠️ | Deferred. |

## 2. Security (Critical Path)

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Server TLS** (Termination) | ✅ | Single cert per listener supported. |
| **Upstream TLS** (Origination) | ✅ | Per-upstream CA bundles and SNI policies supported. |
| **Inbound mTLS** (Client Cert Validation) | ✅ | Enforced via OpenSSL backend. |
| **Outbound mTLS** (Client Cert to Upstream) | ✅ | Client cert + chain modes supported via OpenSSL backend. |
| **RBAC** (Path/Method Auth) | ✅ | Deny-by-default policies. |
| **SNI Multi-Cert** | ❌ | **Dropped**. Sidecars usually have 1 identity. Use Ingress for multi-domain. |
| **External Auth** (OIDC/OAuth) | ❌ | **Dropped**. Sidecar handles Service-to-Service, not End-User Login. |
| **WAF** (ModSecurity) | ❌ | **Dropped**. Performance killer. Use dedicated firewall. |

**Note on TLS Backend**: The runtime is OpenSSL-only; Rustls is not supported or tested in CI.

## 3. Resilience

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Active Health Check** | ✅ | Periodic GET probes; 2xx = healthy. |
| **Outlier Detection** (Passive) | ✅ | Ejects endpoints after consecutive 5xx/transport errors. |
| **Circuit Breaking** | ✅ | Connection limits (`pool.max`) enforced with semaphore-based gating. Queue capacity and timeout supported. |
| **Fault Injection** | ⚠️ | Deferred. For chaos engineering only. |

## 4. Observability

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Prometheus Metrics** | ✅ | Request/upstream metrics with bounded cardinality controls. |
| **Access Logs** (JSON) | ✅ | Structured logging with request IDs and timing metadata. |
| **Distributed Tracing** (OTLP) | ✅ | OpenTelemetry spans with HTTP semantic conventions. |
| **Tap / Packet Capture** | ❌ | **Dropped**. Use system tools (`tcpdump` / `eBPF`). |

## 5. Operational Lifecycle

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Graceful Shutdown** | ✅ | Configurable drain timeout for in-flight requests (SIGTERM/SIGINT). |
| **Admin API** | ✅ | Read-only endpoints for health checks (`/health`) and runtime stats (`/stats`). |
| **Hot Reload** | ✅ | Atomic configuration swapping via pointer replacement (no connection drops). |

## 6. Extensibility

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Wasm Plugins** | ❌ | **Dropped**. High complexity/overhead. |
| **Lua Scripting** | ❌ | **Dropped**. Unpredictable latency. |
| **gRPC Transcoding** | ❌ | **Dropped**. Use a dedicated gateway or generated clients. |

## 7. Tooling & QA

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Benchmark/Test Context Artifacts** | ✅ | `context.env` contracts for run-scoped + case-scoped metadata. |
| **Benchmark CPU Pinning & Memory Limits** | ⚠️ | Linux uses `taskset` and memory limits; non-Linux hosts skip both with a warning. |
| **Benchmark Case Defaults** | ✅ | Standalone cases default to `bench/docker-compose.yaml` and `bench/scripts/pretty.sh`. |

## 8. Service Mesh & Integrations

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **xDS Support** (ADS) | ⚠️ | Deferred. Blocked by Security & Observability work. |
| **Kubernetes Operator** | ⏳ | Planned. For native CRD management. |
