# Pavis Code Review & Action Plan

**Date:** 2025-12-26
**Status:** Refactoring Phase Complete

## ✅ Resolved / Completed

### 1. Architectural Refactoring
- **Modularization:** Codebase split into domain-specific modules (`config`, `router`, `upstream`, `telemetry`, `proxy`) within a library crate.
- **Configuration Validation:** Implemented `Config::validate()` returning a `ValidatedConfig` type wrapper, ensuring only valid configs are used at runtime.
- **Performance:**
    - **Regex Pre-compilation:** Regexes are compiled once during `Router` initialization, not per-request.
    - **False Sharing Mitigation:** Upstream atomic counters are now `#[repr(align(64))]` to prevent cache-line contention.
- **Telemetry:** Access logging is now fully asynchronous and non-blocking using a background task and `try_send`.
- **Documentation:** Module boundaries explicitly document architectural invariants (e.g., "No Business Logic in Proxy").

## 🔴 High Priority (Next Steps)

### 1. Implement Upstream TLS Support
- **Location:** `crates/pavis/src/proxy.rs`, `crates/pavis/src/config/mod.rs`
- **Why:** Currently, all upstream connections are plaintext (HTTP).
- **How:**
    - Add `tls: Option<UpstreamTlsConfig>` to `Upstream` struct.
    - Update `upstream_peer` to initialize `HttpPeer` with TLS settings if enabled.

### 2. Parse Duration Strings
- **Location:** `crates/pavis/src/config/mod.rs`
- **Why:** `HealthCheck.interval` and timeouts are raw strings or integers.
- **How:** Use `humantime-serde` to parse "5s", "100ms" into `std::time::Duration` for type safety.

### 3. Security: Input Validation
- **Location:** `crates/pavis/src/proxy.rs`
- **Why:** Prevent header injection and other attacks.
- **How:** Sanitize user-provided header values in `apply_request_headers`.

## 🟡 Medium Priority

### 4. Use Pingora ServerConf
- **Location:** `crates/pavis/src/main.rs`
- **Why:** Currently manually patching `server.configuration`. Should construct `pingora::server::configuration::ServerConf` properly.

### 5. Access Log Configuration Cleanup
- **Location:** `crates/pavis/src/config/mod.rs`
- **Why:** `AccessLogConfig::False` is unconventional. Rename to `Disabled` and ensure case-insensitive deserialization.

### 6. Handle `respond_error` Result
- **Location:** `crates/pavis/src/proxy.rs`
- **Why:** Errors sending 404/500 responses are currently ignored (`let _ = ...`). Should be logged.

## 🟢 Low Priority

### 7. Support Multiple Listen Addresses
- **Location:** `crates/pavis/src/main.rs`
- **Why:** Support listening on multiple interfaces/ports.

### 8. Config Hot Reload
- **Location:** `crates/pavis/src/main.rs`
- **Why:** Allow config updates without restarting the process.

## 🧪 Missing Tests
- **Error Response Handling:** Unit tests for internal error mapping.
- **Access Log Failures:** Tests for log rotation or disk full scenarios (hard to test deterministically).