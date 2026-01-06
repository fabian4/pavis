# Detailed E2E Test Plan: TLS, DNS, and L7 Routing

This document specifies the end-to-end test cases required to validate the implementation of the new TLS, Advanced DNS, and L7 Traffic Management features. These tests will be implemented in `pavis-e2e`.

## 1. TLS Termination Tests

**Goal:** Verify that Pavis can successfully terminate TLS traffic using file-based certificates and correctly proxy the decrypted requests to upstreams.

### Test Case: `e2e_tls_termination_success`
*   **Description:** Validates basic HTTPS termination with a valid certificate chain.
*   **Setup:**
    1.  Generate a self-signed Root CA (`root.crt`, `root.key`).
    2.  Generate a server certificate (`server.crt`, `server.key`) signed by the Root CA for `localhost`.
    3.  Create a Pavis config with a listener on port `0` (random) enabling TLS with these file paths.
    4.  Start a plaintext backend server (Upstream) on `127.0.0.1:0`.
*   **Action:**
    1.  Start Pavis.
    2.  Create a `reqwest::Client` configured to trust `root.crt`.
    3.  Send an HTTPS request to Pavis: `https://localhost:<port>/path`.
*   **Assertion:**
    1.  Client receives `200 OK`.
    2.  Backend receives the request (decrypted).
    3.  Backend sees the correct path `/path`.

### Test Case: `e2e_tls_termination_invalid_client`
*   **Description:** Ensures Pavis rejects clients that do not trust the certificate (standard TLS behavior, verifying the handshake isn't bypassed).
*   **Setup:** Same as above.
*   **Action:**
    1.  Create a `reqwest::Client` *without* trusting the custom Root CA (default system roots only).
    2.  Send an HTTPS request to Pavis.
*   **Assertion:**
    1.  Client request fails with a TLS/SSL error (e.g., `CERTIFICATE_VERIFY_FAILED`).
    2.  Backend receives **no** connection.

### Test Case: `e2e_tls_missing_files_fail_fast`
*   **Description:** Verifies that Pavis refuses to start if certificate files are missing.
*   **Setup:**
    1.  Create a Pavis config referencing non-existent paths `/tmp/missing.crt`.
*   **Action:**
    1.  Attempt to spawn the Pavis process.
*   **Assertion:**
    1.  Process exit code is non-zero.
    2.  Stderr contains a clear error message (e.g., "Failed to load certificate").

## 2. Advanced DNS Tests

**Goal:** Verify the behavior of `StrictDns` and `LogicalDns` modes, focusing on dynamic resolution and TTL observance.

### Test Case: `e2e_strict_dns_resolution`
*   **Description:** Verifies that `StrictDns` resolves hostnames to IPs and load balances across them.
*   **Setup:**
    1.  Use a mock DNS server (e.g., `trust-dns-server` or similar in-process mock) binding to a random port.
    2.  Configure Mock DNS to resolve `backend.local` to `127.0.0.1` and `127.0.0.2` (TTL 1s).
    3.  Start Pavis with an Upstream using `StrictDns` for `backend.local`.
*   **Action:**
    1.  Send repeated requests to Pavis.
*   **Assertion:**
    1.  Pavis successfully connects to `127.0.0.1` and `127.0.0.2` (assuming backend listeners exist there).
    2.  Wait 2s (TTL expiry).
    3.  Update Mock DNS to return `127.0.0.3`.
    4.  Send requests.
    5.  Pavis connects to `127.0.0.3`.

### Test Case: `e2e_logical_dns_lazy_resolution`
*   **Description:** Verifies that `LogicalDns` resolves IPs lazily during the request filter phase.
*   **Setup:**
    1.  Start Pavis with an Upstream using `LogicalDns` for `dynamic.local`.
    2.  Mock DNS is initially responding `NXDOMAIN`.
*   **Action:**
    1.  Send request. Should fail (502/503).
    2.  Update Mock DNS to resolve `dynamic.local` to `127.0.0.1`.
    3.  Send request immediately.
*   **Assertion:**
    1.  The second request succeeds. (Demonstrates resolution happens *at request time* or shortly after, not requiring a config reload).

## 3. L7 Traffic Management Tests

**Goal:** Verify routing actions (Redirect, DirectResponse) and rewrite policies (Prefix, Host).

### Test Case: `e2e_action_redirect`
*   **Description:** Verifies 3xx redirection logic.
*   **Setup:**
    1.  Config: Route `/old-path` -> Action `Redirect { status: 301, location: "https://new-site.com/resource" }`.
*   **Action:**
    1.  Client sends `GET /old-path`.
*   **Assertion:**
    1.  Client receives `301 Moved Permanently`.
    2.  Response header `Location` is `https://new-site.com/resource`.
    3.  **No** traffic is sent to any upstream.

### Test Case: `e2e_action_direct_response`
*   **Description:** Verifies synthetic response generation.
*   **Setup:**
    1.  Config: Route `/healthz` -> Action `DirectResponse { status: 200, body: "OK" }`.
*   **Action:**
    1.  Client sends `GET /healthz`.
*   **Assertion:**
    1.  Client receives `200 OK`.
    2.  Response body is `OK`.
    3.  **No** traffic is sent to any upstream.

### Test Case: `e2e_rewrite_prefix_preserves_query`
*   **Description:** Verifies path prefix rewriting correctly handles query strings.
*   **Setup:**
    1.  Config: Route `/api/v1/` -> Forward to Backend, Rewrite Prefix `/`.
    2.  Backend server echoes the received path and query string.
*   **Action:**
    1.  Client sends `GET /api/v1/users?sort=asc&limit=10`.
*   **Assertion:**
    1.  Backend receives request for `/users?sort=asc&limit=10`.
    2.  (Crucially verifies that `?sort=asc&limit=10` was not stripped during the rewrite).

### Test Case: `e2e_rewrite_host_header`
*   **Description:** Verifies the `Host` header is modified before going upstream.
*   **Setup:**
    1.  Config: Route `/` -> Forward to Backend, Rewrite Host `internal.service`.
    2.  Backend server echoes the received `Host` header.
*   **Action:**
    1.  Client sends request with `Host: public.api.com`.
*   **Assertion:**
    1.  Backend receives request with `Host: internal.service`.

### Test Case: `e2e_traffic_splitting`
*   **Description:** Verifies weighted traffic distribution across multiple destinations.
*   **Setup:**
    1.  Start two backends: `ServiceA` and `ServiceB`.
    2.  Config: Route `/` -> Forward `[ {upstream: ServiceA, weight: 80}, {upstream: ServiceB, weight: 20} ]`.
*   **Action:**
    1.  Send 100 requests.
*   **Assertion:**
    1.  `ServiceA` receives roughly 80 requests (allow standard deviation tolerance).
    2.  `ServiceB` receives roughly 20 requests.
