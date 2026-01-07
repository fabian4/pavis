# Audit Phase 3: Error Model & Diagnostics
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. Error Inventory & Handling

The runtime employs a robust error handling strategy consistent with high-reliability data planes.

*   **Request-Time Errors**:
    *   Failures during request processing (e.g., "No upstream selected", "Upstream has no endpoints") are captured using Pingora's `Error` type.
    *   These errors result in appropriate HTTP status codes (e.g., 502 Bad Gateway) being returned to the client, rather than crashing the process.
*   **Configuration Errors**:
    *   Invalid headers in the configuration are caught gracefully at runtime in `header_ops.rs`. Instead of panicking, the runtime logs a warning via `tracing` and skips the invalid header operation. This creates a resilient "best effort" behavior for minor configuration defects.
*   **Startup Errors**:
    *   Fatal errors during startup (e.g., invalid CLI args, unreadable LKG file) use `anyhow::Result` to propagate failure to `main`, printing a clean error message and exiting with non-zero status.

## 2. Context & Diagnostics

The telemetry integration (`tracing`) provides high-quality structured logs.

*   **Structured Fields**: Log events consistently include key identifiers:
    *   `upstream`: The name of the target upstream cluster.
    *   `endpoint`: The specific IP:Port selected.
    *   `host`: The Host header / SNI.
    *   `route`: The matching route path/regex.
    *   `error`: The underlying error cause.
*   **Example**:
    ```rust
    tracing::warn!(error = %err, rewrite = %to.0, "Failed to apply path rewrite");
    tracing::debug!(upstream = %upstream_name.0, endpoint = %addr, "forwarding request");
    ```

## 3. Panic Policy Verification

A rigorous search for `unwrap()`, `expect()`, `panic!`, `todo!`, and `unreachable!` was performed.

*   **Production Code**: **Clean**.
    *   **Zero Panics**: No instances of panic-inducing calls were found in the hot request path (`src/proxy`, `src/router`, `src/upstream`).
    *   **Exceptions**:
        *   `src/state.rs`: `expect("empty router")` is used during static `Default` initialization of an empty vector. This is provably safe.
        *   `src/proxy/service.rs`: `unwrap_or_else` is used to provide a default SNI ("localhost"), which is safe control flow, not a panic.
        *   `src/telemetry/access_log.rs`: `unwrap_or` is used for log formatting defaults.
*   **Test Code**: Extensive use of `unwrap()` exists in `#[cfg(test)]` blocks, which is standard practice for test assertions.

**Verdict**: **PASS**. The runtime exhibits excellent error discipline, prioritizing availability and diagnosability.
