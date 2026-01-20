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
| **L7 Routing** (Path/Header/Method) | 🚧 | Path routing works. Method/Header matching has known gaps. |
| **Traffic Splitting** (Canary) | ✅ | Weighted round-robin supported. |
| **Header Manipulation** | ✅ | Add/Remove headers supported. |
| **Redirect & DirectResponse** | ✅ | Supported. For HTTP->HTTPS or security blocking. |
| **Rewrite** (Host/Path) | ✅ | Prefix & Host literal supported. (No Regex rewrite). |
| **Retries & Timeouts** | ✅ | Route-level retries and per-try timeouts are enforced. |
| **Traffic Mirroring** (Shadowing) | ⚠️ | Deferred. Not critical for MVP. |
| **Global Rate Limiting** | ❌ | **Dropped**. Too heavy (requires ext Redis/gRPC). Use Ingress. |
| **Local Rate Limiting** | ⚠️ | Deferred. |

## 2. Security (Critical Path)

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Server TLS** (Termination) | ✅ | Single cert per listener supported. |
| **Upstream TLS** (Origination) | 🚧 | Supported. Custom CAs blocked on rustls backend (P0). |
| **Inbound mTLS** (Client Cert Validation) | ⚠️ | Requires OpenSSL/BoringSSL backend. Not available with rustls. |
| **Outbound mTLS** (Client Cert to Upstream) | ⚠️ | Requires OpenSSL/BoringSSL backend for custom CAs. Not available with rustls. |
| **RBAC** (Path/Method Auth) | ✅ | Deny-by-default policies. |
| **SNI Multi-Cert** | ❌ | **Dropped**. Sidecars usually have 1 identity. Use Ingress for multi-domain. |
| **External Auth** (OIDC/OAuth) | ❌ | **Dropped**. Sidecar handles Service-to-Service, not End-User Login. |
| **WAF** (ModSecurity) | ❌ | **Dropped**. Performance killer. Use dedicated firewall. |

**Note on TLS Backend**: Feature availability depends on the compile-time backend (Rustls vs OpenSSL). See `docs/OPERATIONS.md`.

## 3. Resilience

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Active Health Check** | ✅ | Periodic GET probes; 2xx = healthy. |
| **Outlier Detection** (Passive) | ✅ | Ejects endpoints after consecutive 5xx/transport errors. |
| **Circuit Breaking** | 🚧 | Connection limits (`pool.max`) parsed but unenforced (P0). |
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
