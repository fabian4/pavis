# Relay E2E Cases

Source: `crates/pavis-e2e/tests/relay/relay.rs`

## Publish validation and headers
- Test: `crates/pavis-e2e/tests/relay/relay.rs` (`relay_publish_validation_and_headers`)
- Setup: relay starts with empty LKG and custom headers enabled.
- Requests and assertions:
  - POST `/v1/publish` with empty body -> 400.
  - POST `/v1/publish` without `X-Pavis-Version` -> 400.
  - POST `/v1/publish` with invalid payload -> 422.
  - POST `/v1/publish` with valid payload -> 200.
  - GET `/v1/config` with `X-Pavis-Version: 0` -> 200 and:
    - `X-Pavis-Version: 1`
    - `X-Pavis-Checksum` present
    - `X-Pavis-Checksum-Alg` present
    - `Content-Type: application/octet-stream`
    - `Cache-Control: no-store`
    - body is valid `.pvs`

## Long-poll update
- Test: `crates/pavis-e2e/tests/relay/relay.rs` (`relay_long_poll_updates`)
- Setup: publish version 1, then issue long-poll request with version 1.
- Requests and assertions:
  - GET `/v1/config?wait_ms=2000` with `X-Pavis-Version: 1` blocks.
  - POST `/v1/publish` with version 2 unblocks the wait.
  - Response is 200 with `X-Pavis-Version: 2` and body is valid `.pvs`.

## LKG persistence across restart
- Test: `crates/pavis-e2e/tests/relay/relay.rs` (`relay_persists_lkg_across_restart`)
- Setup: publish version 3 and verify LKG file exists on disk.
- Steps and assertions:
  - Restart relay process/container.
  - GET `/ready` -> 200.
  - GET `/v1/config` with `X-Pavis-Version: 1` -> 200 (no long-poll hold).
  - Response header `X-Pavis-Version` is `0` (LKG is active).
  - Response body is valid `.pvs`.
