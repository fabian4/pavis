# Integrated E2E Cases

## Integrated Plan (Relay + Pavis)

### I1: Publish -> long-poll -> runtime apply
- Setup: relay, runtime, two upstreams A/B.
- Publish artifact v1: route `/` -> A.
- Runtime long-polls `/v1/config`.
- Expect:
  - First request to runtime `/` returns "A".
  - Headers in response or logs include `X-Pavis-Version: 1`.
- Publish artifact v2: route `/` -> B.
- Expect:
  - Runtime receives update; `/` returns "B".
  - `GET /v1/status` shows artifact version 2.

### I2: Invalid publish does not change runtime
- Publish invalid config (unknown upstream).
- Expect: relay rejects or runtime refuses to apply; LKG stays on artifact v2.
- Validate by sending request and checking it still routes to "B".

### I3: Concurrency
- Start 3 runtimes long-polling.
- Publish v1, v2, v3 quickly.
- Expect: all runtimes converge to v3; none apply partial updates.
  - Failure injection strategy (network-based):
    - For one runtime, route relay traffic through a local TCP proxy that
      drops or delays connections for `/v1/config` while other runtimes remain
      connected. Ensure the delayed runtime still converges to v3.

### I4: Observability integration
- For each publish, assert:
  - Relay `/metrics` increments publish count.
  - Runtime `/metrics` increments apply count or config reload count.
  - Headers `X-Pavis-Version` and `X-Pavis-Checksum` present in relay responses.

### I5: File Ingest -> Relay -> Runtime (Pipeline)
- Setup: Relay configured with `file` ingest source watching `input.yaml`.
- Action: Write valid YAML config (v1) to `input.yaml`.
- Expect:
  - Relay logs ingest and successful publish.
  - Runtime picks up v1 via long-poll.
- Action: Update `input.yaml` (v2).
- Expect:
  - Relay version increments automatically.
  - Runtime applies v2.
- Rationale: Verifies the full GitOps-style pipeline from disk to live traffic.

### I6: Data Plane Recovery
- Setup: Relay running with current version v2. Runtime connected.
- Action: Kill Runtime (`pavis`). Restart Runtime.
- Expect:
  - Runtime connects to Relay on boot.
  - Runtime applies v2 immediately (via direct fetch or long-poll).
  - Traffic flow restores successfully.

### I7: Network Partition
- Setup: Stable state.
- Action: Block network between Runtime and Relay (e.g., iptables or disconnect interface).
- Action: Update Relay to v3 (via file ingest or API).
- Action: Restore network.
- Expect:
  - Runtime eventually reconnects/long-polls.
  - Runtime detects v3 and updates.
  - No stale config persists indefinitely.

### I8: Stale Control Plane Rejection (Safety)
- Setup: Runtime running on v10. Relay crashes and loses state (starts fresh at v0/v1).
- Action: Runtime polls fresh Relay (v1).
- Expect:
  - Runtime compares v1 < v10.
  - Runtime **rejects** the update (logs warning about non-monotonic version).
  - Runtime continues serving v10.
- Rationale: Prevents accidental rollbacks or data loss if control plane is reset without restoration.

### I9: TLS Configuration Propagation
- Setup: Relay + Runtime.
- Action: Publish config with a TLS listener enabled (cert/key paths).
- Expect:
  - Runtime applies config and opens the TLS port.
  - Client can successfully establish a TLS connection and receive a response.
- Rationale: Verifies that security configurations are correctly distributed and applied.

### I10: Traffic Management Action Propagation (Redirect/Direct)
- Setup: Relay + Runtime.
- Action: Publish v1 with a redirect rule (`/old` -> 301 `/new`).
- Expect: Runtime returns 301 Moved Permanently with the correct location.
- Action: Publish v2 with a direct response rule (`/status` -> 200 "OK").
- Expect: Runtime returns 200 OK with the configured body immediately.
- Rationale: Ensures L7 actions are correctly interpreted after delivery through the control plane.

### I11: Rewrite Propagation (with Query Preservation)
- Setup: Relay + Runtime.
- Action: Publish config with prefix rewrite (`/api/v1` -> `/v2`).
- Action: Send request `GET /api/v1/resource?query=true`.
- Expect: Backend receives the request at path `/v2/resource?query=true`.
- Rationale: Verifies that path manipulation logic remains consistent when configuration is delivered dynamically.