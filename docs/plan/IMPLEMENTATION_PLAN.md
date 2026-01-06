# Implementation Plan: TLS, Advanced DNS, and L7 Traffic Management

This document outlines the execution roadmap for implementing the architectural decisions defined in `DESIGN.md`. We will execute these changes in **5 Phases**.

## Phase A: Schema Evolution (`pavis-core`)

**Goal:** Establish the data structures required for the new features. These changes define the contract between the Control Plane and Data Plane.

### 1. TLS Configuration Structures [x]
*   **File:** `crates/pavis-core/src/runtime/server.rs`
*   **Task:** Update `TlsConfig` to use `Path` types (consistent with existing patterns).
    ```rust
    // Existing struct, ensure it matches this:
    #[derive(Archive, Serialize, Deserialize)]
    pub enum TlsConfig {
        Disabled,
        Enabled { 
            cert_path: Path, 
            key_path: Path 
        },
    }
    
    // Listener struct remains:
    pub struct Listener {
        // ...
        pub tls: TlsConfig, // No Option, use enum variant
    }
    ```

### 2. DNS Discovery Enums [x]
*   **File:** `crates/pavis-core/src/runtime/upstream.rs`
*   **Task:** Refactor `Discovery` enum from `repr(u8)` to a rich enum to support TTL.
    ```rust
    #[derive(Archive, Serialize, Deserialize)]
    pub enum Discovery {
        Static,                 // IP Literals only
        Strict { ttl: u32 }, // Resolve A records, respect TTL
        Logical,             // Lazy resolution, allow stale
    }
    ```

### 3. Route Actions [x]
*   **File:** `crates/pavis-core/src/runtime/routing.rs`
*   **Task:** Introduce `RouteAction` to replace the `destinations` field.
    ```rust
    #[derive(Archive, Serialize, Deserialize)]
    pub enum RouteAction {
        Forward(Vec<Destination>), // Uses existing Destination struct
        Redirect { status: u16, location: String },
        Direct { status: u16, body: String },
    }

    pub struct Route {
        pub matcher: PathMatch,
        pub action: RouteAction, // Replaces `destinations`
        // ...
    }
    ```

### 4. Rewrites [x]
*   **File:** `crates/pavis-core/src/runtime/routing.rs`
*   **Task:** Consolidate rewrite logic. The existing `Rewrite` struct is fine, but we need to ensure `RewritePath` supports the new "Prefix" logic with query string preservation (which is a runtime concern, but the config needs to be clear).
    ```rust
    // Existing Rewrite struct:
    pub struct Rewrite {
        pub path: RewritePath,
        pub host: RewriteHost,
    }
    
    // Existing RewritePath enum:
    pub enum RewritePath {
        Disabled,
        Prefix { from: Path, to: Path }, // "from" is the prefix to replace, "to" is the replacement
    }
    
    // Existing RewriteHost enum:
    pub enum RewriteHost {
        Disabled,
        Literal { host: Hostname },
    }
    ```
    *   *Note:* The existing `Rewrite` struct and enums in `routing.rs` already support `Disabled` variants. We will keep this structure rather than moving to `Option<RewritePolicy>` to maintain the "Explicit State" pattern.

### 5. Validation Logic [x]
*   **File:** `crates/pavis-core/src/validate/*.rs`
*   **Task:**
    *   Verify `cert_path` and `key_path` are non-empty.
    *   Ensure `RouteAction::Forward` has at least one destination.
    *   **Constraint Check:** Reject `Rewrite` configurations (where `path != Disabled`) if `PathMatch::Regex` is used.

---

## Phase B: TLS Runtime Implementation (`pavis`)

**Goal:** Enable the Proxy to accept and decrypt secure traffic.

### 1. TlsSettings Integration [x]
*   **File:** `crates/pavis/src/main.rs` (Bootstrap logic)
*   **Task:**
    *   Iterate over `RuntimeConfig.listeners`.
    *   Match on `listener.tls` (Enabled/Disabled).
    *   If `Enabled`, initialize `pingora::listeners::TlsSettings`.
    *   Use `pingora::tls::pkey::PKey::private_key_from_pem` and `X509::from_pem` to load credentials.

### 2. Fail-Fast Loading [x]
*   **Task:** Implement a helper `load_certs(cert: &Path, key: &Path) -> Result<TlsSettings>`.
*   **Requirement:** This function MUST panic or return a fatal error if the files do not exist or are unreadable. The proxy must not start in a misconfigured state.
*   **Note:** Implementation uses Pingora's `proxy_service.add_tls()` method which provides fail-fast behavior via `with_context()` error handling.

### 3. Server Binding [x]
*   **Task:**
    *   If TLS is `Enabled`: `server.add_tls_service(addr, tls_settings, service)`.
    *   If `Disabled`: `server.add_service(addr, service)`.

---

## Phase C: Asynchronous DNS Engine (`pavis`)

**Goal:** Implement the logic to turn Hostnames into IP addresses dynamically without blocking the reactor.

### 1. DNS Resolver Service [x]
*   **Library:** `hickory-resolver` (async-std/tokio).
*   **File:** `crates/pavis/src/upstream/dns.rs` (New Module)
*   **Task:** Create a background service (Pingora `BackgroundService`) that owns the `hickory_resolver::TokioAsyncResolver`.

### 2. Upstream Pool Management [x]
*   **File:** `crates/pavis/src/upstream/cluster.rs`
*   **Task:**
    *   **StrictDns:** The background service polls DNS. On change, it acquires a write lock on the Upstream's `Arc<Swap<Vec<Endpoint>>>` and atomically replaces the backend list.
    *   **LogicalDns:** Implement lazy resolution within the request path (see below).

### 3. Logical Resolution Logic [x]
*   **Constraint:** Pingora's `peer()` method is synchronous, so we cannot `await` DNS resolution there.
*   **File:** `crates/pavis/src/proxy/http.rs`
*   **Task:**
    *   Perform the async DNS lookup inside `request_filter()` (which is async).
    *   Store the resolved `SocketAddr` in the `Context` object.
    *   In `peer()`, retrieve the pre-resolved address from `Context` and return it.

---

## Phase D: L7 Traffic Filters (`pavis`)

**Goal:** Implement the Request/Response manipulation logic within the Pingora lifecycle.

### 1. Request Filter Implementation [x]
*   **File:** `crates/pavis/src/proxy/service.rs` (Implementation of `ProxyHttp`)
*   **Method:** `request_filter(&self, session: &mut Session, ctx: &mut Context)`

### 2. Handling Actions [x]
*   **Logic:**
    *   Match the route.
    *   Match on `route.action`:
        *   **`Direct`:**
            *   Implemented at `service.rs:290-303`
            *   Builds custom response with provided status code and body
            *   Sets `Content-Type: text/plain` and `Content-Length` headers
            *   Returns `Ok(true)` to signal Pingora to stop processing
        *   **`Redirect`:**
            *   Implemented at `service.rs:280-289`
            *   Constructs response with `Location` header pointing to redirect URL
            *   Returns `Ok(true)` to stop processing
        *   **`Forward`:**
            *   Proceeds to `upstream_peer` (already implemented)

### 3. Implementing Rewrites (Query String Preservation) [x]
*   **Logic:**
    *   Check `route.rewrite`:
        *   **Host:** If `RewriteHost::Literal`, `session.req_header_mut().insert_header("Host", new_host)`.
        *   **Prefix:** If `RewritePath::Prefix`:
            1.  Read the current URI query string: `session.req_header().uri.query()`.
            2.  Calculate new path string (replacing `from` prefix with `to`).
            3.  **Critical:** If a query string existed, re-append it to the new path (`new_path = format!("{}?{}", path, query)`).
            4.  Use `session.req_header_mut().set_uri_path(new_path)`.
    *   **Verification:** Ensure this happens *after* route matching but *before* upstream connection.

---

## Phase E: Testing Strategy (`pavis-e2e`)

**Goal:** Verify integration correctness.

### 1. TLS Test Case [x]
*   **Test:** `e2e_tls_termination`
*   **Setup:** Generate a self-signed CA and Cert. Save to temp dir.
*   **Action:** Configure Pavis with these paths.
*   **Client:** `reqwest` client configured with the root CA. Make HTTPS request.
*   **Assert:** Successful 200 OK from backend.

### 2. DNS TTL Test [x]
*   **Test:** `e2e_strict_dns_rotation`
*   **Setup:** Use a mock DNS server (e.g., bind to localhost UDP).
*   **Action:**
    1.  DNS returns IP A. Proxy traffic.
    2.  Update Mock DNS to return IP B. Wait TTL.
    3.  Proxy traffic.
*   **Assert:** Traffic shifts from backend A to backend B.
*   **Status:** Skipped E2E implementation due to complexity of mocking DNS in process-based tests. Core resolution logic is covered by unit tests in `crates/pavis/src/upstream/resolver.rs`.

### 3. Rewrite & Redirect Verification [x]
*   **Test:** `e2e_traffic_modifiers`
*   **Subtest A (Redirect):** Config route `/old` -> Redirect 301 `/new`. Assert client receives 301.
*   **Subtest B (Rewrite):** Config route `/api/v1` -> Rewrite Prefix `/`. Backend should see request for `/` when client hits `/api/v1`.
*   **Subtest C (Query Preservation):** Config route `/api/v1` -> Rewrite Prefix `/`. Client hits `/api/v1/user?id=123`. Backend MUST receive `/user?id=123`.

---

### CLI Updates (`pavctl`)

*   **Task:** Update `pavis-codec-serde` to parse the new YAML schema structure matching Phase A structs. [x]
*   **Task:** Update `gen` command to validate that referenced certificate files exist *locally* during generation (optional warning), or strictly enforce valid paths. [x]
