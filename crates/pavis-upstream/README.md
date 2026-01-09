# pavis-upstream
A deterministic HTTP+HTTPS test fixture for Pavis e2e/bench coverage; it is **not** a production data-plane component.

## Non-goals / Guardrails
- Not a production component.
- Must not be depended on by `pavis`, `pavis-relay`, or any runtime crate.
- Only for deterministic e2e/bench validation.
- Not published (`publish = false`).

## Design overview
- **Stability & Control:** Replaces ad-hoc upstream images (`ealen/echo-server`, `openssl s_server`) with a single binary that the repo owns, version-controls, and ships alongside Pavis.
- **Dual listeners:** Always exposes HTTP (`8080`) and HTTPS (`8443`) in one process so origination, TLS, and plaintext cases share infrastructure.
- **Determinism contract:** All JSON responses are schema-stable with lower-cased headers, fixed field ordering, and explicit `null` for missing data (e.g., `tls.version`, `remote_addr`).
- **Replica awareness:** Multiple containers can run simultaneously; each advertises `INSTANCE_ID` so routing/splitting logic can be asserted against `instance_id` instead of container names.

## Capabilities mapped to Pavis features
| Pavis focus | Upstream support | Assertion pattern |
| --- | --- | --- |
| L7 routing & splits | `/echo` on multiple replicas | Compare `instance_id` distribution. |
| Path/header rewrites | `/echo` | Assert `path` and canonical `headers`. |
| Direct responses / status mapping | `/status?code=N` | Ensure Pavis forwards/mutates status as expected. |
| Timeouts & latency | `/delay?ms=N`, `/hang?ms=N` (planned) | Measure response time or assert Pavis returns 504. |
| Retries / outlier detection | `/flaky?fail=N` (stubbed) + `/received` | Count attempts per namespace. |
| Upstream TLS origination | HTTPS `/echo` | Assert `tls.enabled=true`, `tls.sni` matches config. |
| Observability helpers | `/received`, `/reset` (planned) | Inspect per-case traffic history.

## How it’s used in this repo
1. Runner → suite → case hierarchy (see `tests/README.md`) drives the harness.
2. Each shell case configures Pavis, sends traffic, and asserts behavior against this upstream.
3. Every request MUST include `X-Pavis-Test-Run` and `X-Pavis-Test-Case`; pavis-upstream echoes them in responses and keys any stateful counters with them.
4. Case scripts stay linear: harness handles Docker, process lifecycle, and cleanup—cases only orchestrate config, traffic, assertions, and (optionally) scoped `/reset` calls.

## Configuration & runtime model
| Env Var | Default | Description |
| ------- | ------- | ----------- |
| `HTTP_PORT` | `8080` | Plain HTTP listener (0.0.0.0). |
| `HTTPS_PORT` | `8443` | Rustls listener (0.0.0.0). |
| `INSTANCE_ID` | `pavis-upstream` | Replica identity returned by `/id` & `/echo`. |
| `TLS_CERT_FILE` | **REQUIRED** | Absolute/relative path to the PEM certificate chain. |
| `TLS_KEY_FILE` | **REQUIRED** | Absolute/relative path to the PEM private key matching the cert. |

Runtime behavior:
- Both ports are started on boot; health-ready <10ms.
- TLS materials are mandatory. Startup fails immediately if either `TLS_CERT_FILE` or `TLS_KEY_FILE` is missing.
- Recommended docker-compose snippet (after building the local image via `docker build -f crates/pavis-upstream/Dockerfile -t pavis-upstream:local .`):
  ```yaml
  services:
    backend-v1:
      image: pavis-upstream:local
      volumes:
        - ./certs:/certs:ro
      environment:
        - INSTANCE_ID=backend-v1
        - TLS_CERT_FILE=/certs/upstream.crt
        - TLS_KEY_FILE=/certs/upstream.key
  ```

## Determinism & isolation rules
1. **Namespace everything:** `X-Pavis-Test-Run` + `X-Pavis-Test-Case` are mandatory on *all* traffic (including `/reset`).
2. **Stable schema:** Fields never disappear; unknown data is expressed as `null` rather than omitted. Headers are stored as `BTreeMap<String, Vec<String>>` so comparisons are deterministic.
3. **No shared state bleed:** Stateful endpoints (`/flaky`, `/received`, `/reset`) scope data to the namespace headers; never rely on client IP for isolation.
4. **Case hygiene:** Follow the shell structure in the case-authoring guide—use helper scripts, avoid docker/sleep hacks, and write temp files within the runner-provided workspace.

## Endpoint reference
| Endpoint | Method(s) | Status | Purpose / notes |
| -------- | --------- | ------ | --------------- |
| `/healthz` | GET | ✅ | Instant readiness probe returning `{ "ok": true }`. |
| `/id` | GET | ✅ | Replica identity: `{ "id": INSTANCE_ID }`. |
| `/echo` | ANY | ✅ | Reflects method, path, query, canonical headers, TLS metadata, `body_len`, `remote_addr`. |
| `/status?code=N` | GET | ✅ | Returns requested HTTP status plus `{ "status": N, "ok": bool }`. |
| `/delay?ms=N` | GET | ✅ | Sleeps up to 60s then responds `{ "delayed_ms": bounded }`. |
| `/bytes?n=N` | GET | ⚠️ stub | Planned deterministic payload stream; currently `501` stub body. |
| `/hang?ms=N` | GET | ⚠️ stub | Planned long-hold connection for timeout testing. |
| `/close` | GET | ⚠️ stub | Planned TCP reset simulator. |
| `/flaky?fail=N` | GET | ⚠️ stub | Planned fail-then-success counter per namespace. |
| `/received` | GET | ⚠️ stub | Planned observability summary per namespace. |
| `/reset` | POST | ⚠️ stub | Planned state reset for the calling namespace only.

All responses echo the namespace headers when present to simplify traceability.

## Feature-oriented usage patterns
- **Routing / traffic splits:** Hit `/echo` repeatedly, record `instance_id`, and assert distribution (never rely on container order).
- **Header/path rewrites:** Compare `path` and `headers` fields from `/echo` against expected mutations.
- **Retries:** Target `/flaky?fail=1` (once implemented) and verify client sees 200 while `/received` shows >1 attempts.
- **Timeouts:** Combine `/delay` for soft latency assertions and `/hang` (once implemented) for hard client-timeout coverage.
- **Upstream TLS:** Route through HTTPS port, then assert `tls.enabled=true` and `tls.sni` equals configured hostname.

## Case authoring quick reference
- **Script skeleton:** See `tests/README.md` for the canonical bash layout (imports → config → run Pavis → traffic → assertions → optional cleanup).
- **Isolation headers:** Always include both namespace headers in *every* curl/helper invocation—even `/healthz` probes—to avoid state collisions.
- **Forbidden patterns:** No direct Docker invocations, no random ports, no assuming client IP/ordering, no external internet calls.
- **Assertions:** Prefer `jq`/helpers over brittle `grep`; JSON keys are already sorted but treat them as unordered.
- **State cleanup:** Use `/reset` (once implemented) only with the same namespace headers that created the state.

## Local quickstart
```sh
# 1) Generate throwaway certs (one-time for local dev)
mkdir -p tmp/certs
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout tmp/certs/upstream.key \
  -out tmp/certs/upstream.crt \
  -subj "/CN=localhost" -days 1

# 2) Run pavis-upstream using the generated materials
TLS_CERT_FILE=tmp/certs/upstream.crt \
TLS_KEY_FILE=tmp/certs/upstream.key \
INSTANCE_ID=dev-upstream \
HTTP_PORT=8080 HTTPS_PORT=8443 \
cargo run -p pavis-upstream
```

Example probes:
```sh
# HTTP health
curl http://127.0.0.1:8080/healthz

# HTTPS echo (trust the cert via your CA store or use -k for the throwaway cert above)
curl -k -H "X-Pavis-Test-Run: local" -H "X-Pavis-Test-Case: demo" \
  https://127.0.0.1:8443/echo -d '{"hello":"world"}'
```

## CI / E2E integration notes
- Pin image tags when containerizing; avoid floating `latest`.
- Runner healthchecks should use `/healthz` (HTTP) for readiness; optionally expose a CLI `healthcheck` subcommand in the future.
- Harnesses should mount deterministic TLS materials in CI so clients can trust via CA pin rather than `-k`.
- Parallel safety relies entirely on namespace headers—missing headers will cause flaky tests and shared state pollution.

## FAQ / Troubleshooting
- **Why not ealen/echo-server?** We require deterministic schema, namespace-aware state, and dual HTTP/HTTPS listeners that off-the-shelf images don’t provide.
- **Why keep it in the workspace?** Ensures the crate builds with the rest of Pavis, inherits workspace linting policy, and avoids external binary drift.
- **Do we publish it?** No. It remains internal (`publish = false`) and must only ship through this repo.
- **Why can `/flaky` affect other tests?** Without the namespace headers every replica would share counters; always send both headers to isolate effects.
- **How do I validate upstream TLS?** Provide cert/key paths via env vars and ensure clients trust that certificate (install the CA in CI or pin the self-signed fingerprint for local runs).
