# Relay Operational Evidence

This document records how the relay was exercised to prove that artifact distribution stays deterministic and fail-closed. It is not a deployment recipe.

## Scope
- Covers the single binary at `crates/pavis-relay` acting as publisher + LKG store.
- All protocol semantics live in `../specs/relay-protocol.md` and `../api/relay.md`.
- The relay never interprets config content; it only validates `.pvs` headers and persists bytes.

## Experiment Matrix
| ID | Scenario | Setup | Observation |
| -- | -------- | ----- | ----------- |
| REL-01 | Cold start with empty history | `pavis-relay --config relay.yaml --data-dir tmp/relay` | Creates `history/0000000001.*` and `lkg/` after first publish; `/v1/status` reports version `1`. |
| REL-02 | Long-poll fanout | Start runtime clients with `If-None-Match` headers while publishing new artifact | Clients unblock with `200` responses carrying ETAG; relay logs `longpoll_awakened count=N`. |
| REL-03 | Corrupt upload | Flip one byte in payload before POST | Relay responds `400` with `error=invalid-artifact`, history untouched, `pavis_relay_publish_fail_total` increments. |
| REL-04 | Disk persistence loss | Delete `history/` while relay running | Relay rebuilds `history/` lazily on next publish; LKG copy remains authoritative. |
| REL-05 | Relay restart | Kill process mid-long-poll, restart | Clients reconnect, ETAG preserved, `lkg/config.pvs` re-verified on boot. |

## Storage Model
```
<root>/
  lkg/
    config.pvs        # atomically replaced only after validation succeeds
    meta.json         # checksum + monotonic version
  history/
    0000000001.pvs    # full artifact log
    0000000001.meta.json
```
- History files provide auditability; runtime LKG reads never touch them.
- Version numbers are monotonically increasing `u64`; relay refuses to decrement or skip.

## Failure Handling
- Publish requests stream directly to a staging file; verification occurs before the file is moved into `history/`.
- If verification fails, the staging file is deleted and no observable state changes.
- If disk fsync fails, the HTTP request fails with `500` and the caller is expected to retry.
- Long-poll connections are terminated immediately when a new version is committed; clients must handle reconnects.

## Observability Surfaces
- `/health` returns `200` iff HTTP listener is bound and LKG metadata loaded.
- `/v1/status` exposes `{current_version, history_len, checksum}` for audit.
- Metrics:
  - `pavis_relay_publish_ok_total`
  - `pavis_relay_publish_fail_total`
  - `pavis_relay_longpoll_wait_total`
  - `pavis_relay_disk_fsync_seconds`

## Reproduction Notes
- Use `pavctl publish --relay http://127.0.0.1:8080 artifact.pvs` for deterministic publishes.
- For corruption tests, `dd if=/dev/zero of=artifact.pvs bs=1 seek=16 count=1 conv=notrunc` flips a byte before POST.
- To observe long-poll behavior, issue `curl -N -H "If-None-Match: <etag>" http://127.0.0.1:8080/v1/config?wait_ms=30000` from multiple terminals.

## Related References
- `../specs/relay-protocol.md` — canonical protocol definition.
- `../api/relay.md` — HTTP surface used in the experiments.
- `/ARCHITECTURE.md` — describes why relay stays policy-agnostic.
