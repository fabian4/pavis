# Failure & Recovery Semantics

This document captures the experiments used to show that the runtime fails closed, never mutates state on partial inputs, and always returns to the Last Known Good (LKG) snapshot. It replaces the previous SRE-style runbook.

## Guiding Principles
1. All state derives from sealed `.pvs` artifacts. The runtime never mutates config in-place.
2. Reload attempts are transactional: stage → verify → commit. Any failure aborts the swap and retains LKG.
3. Only execution-environment failures are detected here (filesystem, sockets, TLS keys). Semantic validity is decided earlier in the pipeline.

## Evidence Checklist
| ID | Injection | Expected Result | Observed |
| -- | --------- | --------------- | -------- |
| F-01 | Corrupt artifact header | Reload rejected, LKG untouched, `runtime.reload.failure_total` increments. | ✔ |
| F-02 | Listener port occupied | Startup aborts before traffic starts; exit code `1`; no listeners bind partially. | ✔ |
| F-03 | TLS key unreadable | Startup aborts with `error=tls_key_io`; admin server never opens. | ✔ |
| F-04 | DNS returns empty set | Resolver keeps previous endpoints (`lkg_endpoint_retained=true`) and logs warning. | ✔ |
| F-05 | Relay unreachable during watch | Runtime keeps existing artifact, retries with exponential backoff; no config divergence. | ✔ |
| F-06 | SIGKILL during reload | On restart, runtime loads LKG from disk and rejects partially written staging file. | ✔ |

## Recovery Workflow (Conceptual)
1. **Detect Failure** — via non-zero exit code, admin `/health` returning `500`, or relay publish rejection.
2. **Validate Artifact** — `pavis-pvs verify <path>` confirms that the bytes are well-formed.
3. **Restore LKG** — copy a known-good artifact (from relay history or version control) over the runtime's configured path.
4. **Restart Runtime** — supervisor restarts the binary; runtime validates again and binds listeners.

No additional knobs or "repair" commands exist; restoration always involves supplying a trusted artifact.

## Supervisors & Signals
- Supervisors (systemd, Kubernetes, docker) are outside this project’s scope. They only restart the binary or forward POSIX signals.
- `SIGTERM` triggers graceful shutdown, honoring `graceful_shutdown_timeout_seconds` from the artifact. If the timer elapses, remaining connections are closed.
- `SIGHUP` triggers a reload from disk even without relay involvement; failure leaves state untouched.

## Artifact Provenance
- The runtime keeps an on-disk LKG copy under `state/lkg/config.pvs`. On boot it verifies checksum + magic bytes before constructing routers.
- Relay history can be queried for any previous version via `/v1/artifacts/<version>` and used to repopulate LKG.

## Operational Boundaries
- No auto-mitigation. If a config is invalid, the runtime refuses to serve it.
- No partial listener startup. Either all listeners/admin/metrics bind, or the process exits with error.
- No runtime defaults. Every timeout, retry, or rewrite policy must already exist in the artifact; otherwise startup fails earlier in the pipeline.

## Related Material
- `/ARCHITECTURE.md` — frozen data plane axioms.
- `runtime-config-fsm` spec — describes the loader states referenced above.
- `docs/operations/runtime.md` — provides the complementary positive-path evidence.
