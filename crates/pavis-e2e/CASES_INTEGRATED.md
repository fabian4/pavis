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
