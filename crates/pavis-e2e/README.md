# Pavis E2E Test Design

This document defines the end-to-end (E2E) testing plan for Pavis across
runtime, relay, and integrated flows. It is prescriptive about endpoints,
signals, and expected behavior, but does not include full production code.

Related case docs:
- `crates/pavis-e2e/CASES_PAVIS.md`
- `crates/pavis-e2e/CASES_RELAY.md`
- `crates/pavis-e2e/CASES_INTEGRATED.md`

## Scope And Boundaries

- Runtime (`pavis`) consumes only `.pvs` artifacts via `pavis-pvs` and accepts
  only `ValidatedRuntimeConfig`. Runtime MUST NOT parse DTOs.
- Relay (`pavis-relay`) owns ingest + codec orchestration, versioning, last
  known good (LKG), and artifact distribution.
- Codecs (`pavis-codec-*`) parse and validate DTOs, producing
  `ValidatedRuntimeConfig`. Codec has no I/O.
- `.pvs` integrity is enforced in `pavis-pvs` (magic/version/checksum).

## Execution Modes

E2E tests run in two modes:
- Binary mode: local binaries, dockerized upstreams.
- Docker mode: relay/runtime in containers, dockerized upstreams.

Mode is selected via `TEST_MODE=binary|docker`.

## Fixtures And Artifacts

- `.pvs` is produced by test helpers and published via relay.
- Relay owns LKG and artifact storage in a test temp dir.
- Runtime reads LKG from disk and does not parse DTOs.

## Endpoints

Relay:
- `POST /v1/publish`
- `GET /v1/config` (long-poll)
- `GET /v1/artifacts/:version`
- `GET /v1/status`
- `GET /v1/metrics` (or `/metrics`)
- `GET /health`
- `GET /ready`

Runtime:
- `GET /v1/status`
- `GET /v1/metrics` (or `/metrics`)
- `GET /health`
- `GET /ready`

If runtime does not expose `/v1/status`, use `/metrics` plus logs for version
visibility.

### Relay Single Source Authority (SSA)

Relay exposes exactly one active Artifact Version at a time. Partial or
parallel activation is forbidden; all consumers must converge to the same
single active version.

## Terminology

- Artifact Version: the monotonic version assigned by relay to each accepted
  artifact (`X-Pavis-Version`). This is the version used by
  long-polling and status endpoints.
- PVS Schema Version: the `.pvs` format version embedded in the `.pvs` header.
  This is validated by `pavis-pvs` and is independent of Artifact Version.

### Relay Long-Poll Contract (GET /v1/config)

Client MUST send the current artifact version in the request header:
- `X-Pavis-Version: <artifact_version>`

Server behavior:
- `200 OK`: there is a newer artifact version; response body is the full `.pvs`
  bytes for the newest version. Response MUST include headers:
  - `X-Pavis-Version: <u64>`
  - `X-Pavis-Checksum: <hex>`
- `304 Not Modified`: long-poll timed out and no newer version exists.
- `400 Bad Request`: missing or invalid `X-Pavis-Version`.

### Publish Contract (POST /v1/publish)

Publish accepts `.pvs` bytes. Relay verifies checksum and schema via
`pavis-pvs`, then versions/stores/distributes the result.

Request:
- Body: raw `.pvs` bytes.
- Headers:
  - `X-Pavis-Version: <u64>`

Response (on success):
- `200 OK`
- Headers:
  - `X-Pavis-Version: <u64>`
  - `X-Pavis-Checksum: <hex>`

On validation failure (checksum/schema): `422` with error message.

### Artifact Fetch Contract (GET /v1/artifacts/:version)

- `200 OK`: response body is the full `.pvs` bytes for the requested artifact
  version, with `X-Pavis-Version` and `X-Pavis-Checksum` headers.
- `404 Not Found`: requested artifact version does not exist.

## Observability Contract

Relay `GET /v1/status` MUST include:
- `version=<u64>`
- `checksum=<hex>`

Runtime `GET /v1/status` JSON MUST include:
- `current_artifact_version` (u64)
- `last_error` (string or null)

Relay `/metrics` MUST include counters:
- A counter indicating successful publish operations.
- A counter indicating failed publish operations.
- A counter indicating long-poll waits/timeouts.

Runtime `/metrics` MUST include counters:
- A counter indicating successful config application.
- A counter indicating failed config application.

## Test Matrix

| ID | Component(s) | Category | Invariant / Boundary |
| --- | --- | --- | --- |
| R1 | Relay | Artifact lifecycle | Monotonic artifact versioning; LKG atomic update |
| R2 | Relay | Integrity | `.pvs` verify: magic/version/checksum enforced by `pavis-pvs` |
| R3 | Relay | Long-poll | Immediate response on new version; timeout behavior |
| R4 | Relay | Failure | Partial write failures; maintain LKG |
| R5 | Relay | Observability | `/v1/status`, `/metrics` reflect versions and counts |
| P1 | Pavis | Integrity | Reject invalid `.pvs` (magic/version/checksum mismatch) |
| P2 | Pavis | Runtime apply | Apply new config via restart with new `.pvs` |
| P3 | Pavis | Failure | Startup fails fast on unreadable or corrupt `.pvs` |
| P4 | Pavis | Compaction | Off/Trim/Prune semantics equal (routing behavior) |
| I1 | Integrated | Long-poll | Relay publish -> runtime fetch -> routes update |
| I2 | Integrated | Failure | Relay publishes invalid config -> runtime stays on LKG |
| I3 | Integrated | Concurrency | Multiple runtimes long-polling; single-writer publish |
| I4 | Integrated | Observability | Version and checksum visible in headers/logs/metrics |

## Coverage Mapping

- Artifact lifecycle: R1, R4
- Integrity: R2, P1
- Long-poll behavior: R3, I1
- Runtime apply semantics: P2, I1
- Failure modes: P3, I2
- Compaction levels: P4
- Observability: R5, I4
- Concurrency: I3

## Implementation Plan (Rust Integration Tests)

Shared fixtures:
- Temp dirs via `tempfile::TempDir`.
- Ports from a test allocator to avoid conflicts.
- Upstreams: spawn lightweight HTTP servers in-test (hyper or tiny-http)
  returning fixed body strings.
- DTO fixtures (YAML or JSON) under `crates/pavis-e2e/fixtures/`.

Suggested endpoint usage:
- Publish: `POST /v1/publish` with `.pvs` bytes and `X-Pavis-Version`.
- Long-poll: `GET /v1/config?wait_ms=...` with `X-Pavis-Version`.
- Artifact fetch: `GET /v1/artifacts/:version`.
- Status: `GET /v1/status` returns text with version/checksum fields.
- Metrics: `GET /v1/metrics`.

Example test flow (pseudocode):

```
let relay = spawn_relay(temp_dir, relay_port);
let upstream_a = spawn_upstream(a_port, "A");
let upstream_b = spawn_upstream(b_port, "B");
publish_artifact(relay, dto_yaml_bytes(config_to_a))?;
let pavis = spawn_pavis(runtime_port, relay_url);

assert_eq!(http_get(runtime_url("/")).body, "A");

publish_artifact(relay, dto_yaml_bytes(config_to_b))?;
wait_for(|| http_get(runtime_status).version == 2, 5s);
assert_eq!(http_get(runtime_url("/")).body, "B");
```

File layout:
- Integrated cases: `crates/pavis-e2e/tests/`.
- Relay-only cases: `crates/pavis-e2e/tests/relay/`.
- Pavis-only cases: `crates/pavis-e2e/tests/pavis/`.

## CI Split And Smoke Suite

Fast (PR):
- Relay-only: R1, R2, R3
- Pavis-only: P1, P2
- Integrated: I1 (single runtime)

Slow (nightly or push to main):
- Integrated: I2, I3
- Compaction: P4
- Failure injection: R4, P3
- Observability checks: R5, I4

Minimal smoke suite:
- One relay-only publish/version test (R1).
- One integrated publish/apply test (I1).

## Harness Options

Option 1: Pure Rust (spawn binaries)
- Approach: `std::process::Command` to run `pavis-relay` and `pavis`,
  `reqwest` for HTTP, temp dirs for state.
- Pros: fast, no Docker dependency, simple local debugging, CI-friendly.
- Cons: harder to model networking failures; needs robust process cleanup.

Option 2: Docker Compose
- Approach: compose file for relay, runtime, upstreams; tests drive HTTP
  against exposed ports.
- Pros: closer to production topology; clean isolation for concurrency tests.
- Cons: slower; Docker dependency in CI; more complex log capture.
