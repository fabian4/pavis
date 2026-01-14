# Pavis Feature Matrix & Envoy Comparison

## Introduction

Pavis is designed as a **Pragmatic & Lightweight Sidecar** for microservices. It is NOT a clone of Envoy and does not aim to support every feature of a general-purpose edge gateway.

Our philosophy focuses on:
*   **Predictable Performance:** Avoiding features that introduce high variance (e.g., regex rewriting, Wasm).
*   **Operational Simplicity:** Reducing the configuration surface area.
*   **Security First:** Enforcing strict defaults for modern service mesh environments.

This document outlines the current and planned feature set, explicitly calling out features that are intentionally dropped to maintain our design goals.

**Legend:**
*   ✅ **Supported**: Implementation is complete and available.
*   ⏳ **Planned**: Currently on the critical path for upcoming releases.
*   ⚠️ **Deferred**: Recognized as valuable but prioritized below critical path items.
*   ❌ **Dropped**: Explicitly out of scope; effectively "WontFix" by design.

---

## 1. Traffic Management

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **L7 Routing** (Path/Header/Method) | ✅ | Based on Pingora engine (Prefix/Exact/Regex). |
| **Traffic Splitting** (Canary) | ✅ | Weighted round-robin supported. |
| **Header Manipulation** | ✅ | Add/Remove headers supported. |
| **Redirect & DirectResponse** | ✅ | Supported. For HTTP->HTTPS or security blocking. |
| **Rewrite** (Host/Path) | ✅ | Prefix & Host literal supported. (No Regex rewrite). |
| **Retries & Timeouts** | ⏳ | Planned. Critical for network stability. |
| **Traffic Mirroring** (Shadowing) | ⚠️ | Deferred. Not critical for MVP. |
| **Global Rate Limiting** | ❌ | **Dropped**. Too heavy (requires ext Redis/gRPC). Use Ingress. |
| **Local Rate Limiting** | ⚠️ | Deferred. |

## 2. Security (Critical Path)

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Server TLS** (Termination) | ✅ | Single cert per listener supported. |
| **Upstream TLS** (Origination) | ⚠️ | Supported. System CA bundle only (rustls). Custom CAs require OpenSSL backend. |
| **Inbound mTLS** (Client Cert Validation) | ⚠️ | Requires OpenSSL/BoringSSL backend. Not available with rustls. |
| **Outbound mTLS** (Client Cert to Upstream) | ⚠️ | Requires OpenSSL/BoringSSL backend for custom CAs. Not available with rustls. |
| **RBAC** (Path/Method Auth) | ✅ | Deny-by-default policies. |
| **SNI Multi-Cert** | ❌ | **Dropped**. Sidecars usually have 1 identity. Use Ingress for multi-domain. |
| **External Auth** (OIDC/OAuth) | ❌ | **Dropped**. Sidecar handles Service-to-Service, not End-User Login. |
| **WAF** (ModSecurity) | ❌ | **Dropped**. Performance killer. Use dedicated firewall. |

**Note on TLS Backend**: The current default build uses Pingora's rustls backend. mTLS features (both inbound client authentication and outbound custom CA verification) require the OpenSSL/BoringSSL backend due to upstream Pingora limitations. Rustls support for these features is blocked on Pingora. See README.md for details.

## 3. Resilience

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Active Health Check** | ⏳ | Planned. Ping `/healthz`. |
| **Outlier Detection** (Passive) | ⏳ | **Critical Gap**. Eject 5xx pods. Key for SLA. |
| **Circuit Breaking** | ⏳ | Planned. Connection limits. |
| **Fault Injection** | ⚠️ | Deferred. For chaos engineering only. |

## 4. Observability

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Prometheus Metrics** | ✅ | Request/upstream metrics with bounded cardinality controls. |
| **Access Logs** (JSON) | ✅ | Structured logging with request IDs and timing metadata. |
| **Distributed Tracing** (OTLP) | ✅ | OpenTelemetry spans with HTTP semantic conventions. |
| **Tap / Packet Capture** | ❌ | **Dropped**. Use system tools (`tcpdump` / `eBPF`). |

## 5. Extensibility

| Feature | Status | Note / Alternative |
| :--- | :---: | :--- |
| **Wasm Plugins** | ❌ | **Dropped**. High complexity/overhead. |
| **Lua Scripting** | ❌ | **Dropped**. Unpredictable latency. |
| **gRPC Transcoding** | ❌ | **Dropped**. Use a dedicated gateway or generated clients. |
