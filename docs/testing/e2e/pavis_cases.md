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
