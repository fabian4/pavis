# Pavis E2E Cases

## Pavis Plan (Runtime-Only)

### P1: Runtime rejects invalid `.pvs`
- Setup: runtime configured to load `.pvs` from file via `--config <path>`.
- Action: provide corrupted `.pvs` (bad magic/checksum).
- Expect: runtime startup fails fast; log contains `checksum mismatch` or
  `invalid header`.

### P2: Runtime apply semantics
- Runtime config apply is restart-based in this phase (no in-process hot reload).
- Start runtime with `.pvs` v1.
- Stop runtime, replace file with `.pvs` v2 (valid), restart runtime.
- Expect: requests switch to new routing on restart; no panic.
- Observables: `GET /v1/status` shows the new artifact version.

### P3: Startup failure on invalid config path
- Failure injection strategy (filesystem-based):
  - Start runtime with a `.pvs` path that does not exist or has `chmod 000`.
- Expect: runtime exits non-zero and logs a clear error.

### P4: Compaction levels
- Same config, compiled with Off/Trim/Prune.
- Request set: GET `/`, GET `/unknown`, GET `/health`, GET `/ready`.
- Expect identical upstream mapping for `/`, identical fallback behavior for
  `/unknown`, and identical status for health/ready.
- Allowed differences: artifact size and byte content; routing semantics MUST
  match for all tested requests.

### P5: TLS Termination
- Setup: Generate self-signed certificates.
- Action: Start runtime with TLS listener enabled using generated certs.
- Client: Use HTTPS client (dangerously accepting invalid certs).
- Expect: 200 OK from backend; traffic is decrypted correctly.

### P6: Redirect & Direct Responses
- Action: Configure exact routes for redirects (301, 302, 307) and direct responses.
- Expect (Redirect): Client receives the correct 3xx status and `Location` header.
- Expect (Direct): Client receives the configured status and body without upstream forwarding.

### P7: Path & Host Rewrites
- Action: Configure prefix rewrite and host rewrite.
- Request: `GET /api/v1/users?id=123`.
- Expect (Path): Backend receives `/v2/users?id=123` (prefix replaced, query preserved).
- Expect (Host): `Host` header updated to match the configured literal.

### P8: DNS Discovery (Logical & Strict)
- Action: Use upstreams with DNS hostnames.
- Expect: Runtime resolves hostnames to IPs and forwards traffic.
- Note: TTL-based rotation is verified via unit tests; E2E verifies basic connectivity to DNS-backed upstreams.

### P9: Basic Routing
- Action: Define multiple upstreams and route traffic to them using prefix matching.
- Expect: Traffic is forwarded to the correct upstream.

### P10: Route Matching (Exact vs Prefix)
- Action: Define routes with exact and prefix matchers.
- Expect: Exact matches take precedence or match specifically; prefix matches handle subpaths.

### P11: Regex Matching
- Action: Define routes using regex matchers.
- Expect: Requests matching the regex patterns are forwarded correctly.

### P12: Wildcard Host Matching
- Action: Define virtual hosts with wildcard domains (e.g., `*.example.com`).
- Expect: Requests with matching Host headers are routed to the wildcard vhost.

### P13: Unmatched Routes
- Action: Send a request that matches no defined route.
- Expect: 404 Not Found.

### P14: Header Manipulation (Request)
- Action: Configure rules to add, set, or remove headers on requests.
- Expect: Backend receives requests with modified headers.

### P15: Response Header Manipulation
- Action: Configure rules to add, set, or remove headers on responses.
- Expect: Client receives responses with modified headers.

### P16: Round Robin Load Balancing
- Action: Configure an upstream with multiple endpoints and Round Robin balancer.
- Expect: Traffic is distributed evenly across endpoints over multiple requests.

### P17: Weighted Traffic Splitting
- Action: Configure a route with multiple destinations and weights (e.g., 80/20).
- Expect: Traffic distribution approximates the configured weights.

### P18: Upstream Weights
- Action: Configure upstream endpoints with different weights.
- Expect: Load balancer respects endpoint weights when distributing traffic.

### P19: HTTP Version
- Action: Configure upstreams with different HTTP protocol versions (H1, H2).
- Expect: Proxy communicates with upstreams using the specified protocol.

### P20: Upstream TLS
- Action: Configure upstream to require TLS (HTTPS).
- Expect: Proxy initiates TLS connection to the upstream.
