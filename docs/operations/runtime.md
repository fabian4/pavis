# Runtime Operational Evidence

This note documents how the frozen data plane runtime was exercised to prove its operational guarantees. It is **not** a deployment guide; it only records the experiments used to show that execution stays dumb, runtime-safe reloads are atomic, and failures stay fail-closed.

## Scope
- Focuses exclusively on runtime behavior after a `.pvs` artifact is handed to the process.
- All semantic guarantees are defined elsewhere (see `/ARCHITECTURE.md` and `../specs/*`).
- Only environment checks allowed at runtime (filesystem readability, socket binding, TLS key access) are covered here.
- The relay-driven fetch loop is executed by a pure FSM + driver split (FSM decides, driver performs I/O).

## Experiment Matrix
| ID | Scenario | Method | Evidence |
| -- | -------- | ------ | -------- |
| R-01 | Cold boot with verified artifact | `pavis --config <artifact>` after `pavis-pvs verify` | Process binds listeners, admin, and telemetry ports; emits `INFO runtime::server started`. |
| R-02 | Reload via sealed artifact swap | Relay-driven fetch + atomic apply | Logs show `config_apply` on success; `RuntimeState` swaps atomically for runtime-safe fields. |
| R-03 | Relay-driven update | Relay publishes new ETAG; runtime long-poll returns `200` with bytes | Runtime writes artifact to LKG staging then repeats R-02 sequence automatically. |
| R-04 | Invalid artifact guard | Corrupt header byte before reload | `ERROR reload_failed` + runtime stays on LKG with unchanged admin counters. |
| R-05 | Listener bind failure | Reserve port with `nc -l 8080` before boot | Startup aborts with `ERROR listener_bind_failed`; no best-effort behavior. |
| R-06 | Graceful shutdown window | Send `SIGTERM` while streaming requests | Logs record `shutdown_initiated` and `shutdown_complete` after `graceful_shutdown_timeout_seconds`; in-flight requests finish, no listeners left open afterward. |

## Environment Checks Performed
The runtime performs only the following environment interactions before executing the compiled plan:
1. `std::fs::File::open` on the artifact path (ensures readability, but does not parse semantics).
2. `pavis_pvs::verify` (magic bytes, version gate, checksum, archive shape).
3. Socket binds for listeners/admin/metrics; failure aborts startup.
4. TLS certificate/private key readability when TLS listeners exist.
5. `BootstrapPlan::build` materializes telemetry, resolvers, admin/metrics, and listener services (via `listener::tls::TlsRuntime`) before `Server::run_forever` executes, so any dependency failure aborts the boot atomically.
6. `RuntimeState::from_config` resolves every DNS endpoint exactly once per reload and stores the resolved sockets inside the upstream manager so request threads never perform DNS.
7. `ClientIdentityMaterializer` loads listener + upstream TLS assets once per reload and shares the resulting identities/CA bundles with both the proxy and health monitor, so probes never touch PEM files after startup.
8. `UpstreamHealthMonitor` converts runtime config into `HealthProbePlan` structs. A scheduler enforces per-upstream intervals and an executor drives probes via `tokio::spawn`, guaranteeing that disabled checks schedule nothing and interval math never drifts under load.

If any step fails, execution halts and the prior Last Known Good (LKG) remains untouched.

Reload additionally enforces a boot-time boundary before swapping state. Changes to `listeners`, `admin`, `shutdown`, `telemetry.metrics`, and `telemetry.access_log` are rejected because those services are still constructed during bootstrap rather than by the live `RuntimeState`.

## Reload / Rollback Semantics
- Reload attempts are serialized. A new artifact is staged under `state::loader`, validated, checked for boot-time field drift, and only then swapped into the live router.
- `MaterializedRuntimeConfig` now captures the router + upstream managers as a unit so proxy threads always see a coherent view of the data plane. Bootstrap-owned services continue to use their startup wiring until a full service-graph reload exists.
- Config versions are represented by the `ConfigVersion` newtype after a versioned artifact is applied; admin and metrics surfaces may still report an unset version during pre-LKG/bootstrap phases.
- If validation fails, the LKG pointer is not advanced and traffic continues on the previous snapshot.
- Rollback is simply "publish the older artifact"; the runtime treats it as any other reload and does not special-case versions.

## Relay Fetch FSM (Operational Summary)
- The runtime agent uses a single-in-flight long-poll loop (`wait_ms = 30000`) and never issues concurrent fetches.
- `204` and `304` are NoUpdate and do not trigger backoff; the next long-poll starts immediately.
- `410` is a NeedResync signal: the agent clears conditional state and immediately performs an unconditional fetch.
- `5xx` and network errors are transient; the agent applies capped exponential backoff before retrying.
- Dedup is checksum-based: artifacts with the same ETag are skipped and never re-applied.

## Failure Injections Observed
| Injection | Expected Outcome | Observed |
| --------- | ---------------- | -------- |
| Truncated artifact | Reload rejected with `CodecError::ArtifactShortRead`; routes keep serving previous snapshot. | ✔ |
| Broken TLS key path | Startup aborts before binding; admin/metrics ports never open. | ✔ |
| Relay 304 long-poll loop | Runtime sleeps until timeout and re-issues GET; no CPU growth. | ✔ |
| Concurrent SIGHUP + Relay publish | Internal queue deduplicates; only the freshest artifact is applied. | ✔ |

## Telemetry Surfaces
- Admin `/health` returns `200` when at least one listener + router is live.
- Admin `/stats` exposes `config_version`, listener counts, and uptime (see `crates/pavis/src/admin.rs`).
- Metrics port is served by `telemetry::metrics::PrometheusEndpoint`, which uses a pluggable transport (Tokio TCP in production) so tests can stub the listener. If the exporter fails to install or the bind fails, startup aborts before any traffic flows.
- Metrics port exports:
  - `pavis_runtime_config_version` (gauge with `version` label)
  - `pavis_runtime_reload_count_total` (counter)
  - `pavis_runtime_reload_last_timestamp` (gauge)
  - `pavis_config_validation_total` / `pavis_config_apply_total` (counters)

## Reproduction Notes
- All experiments were executed with artifacts compiled via `pavctl compile` and verified via `pavis-pvs verify`.
- Relay experiments used `pavis-relay` in loopback mode with an empty history directory to show LKG creation.
- The runtime intentionally provides **no** automation for provisioning, orchestration, or graceful degradation; external supervisors (systemd, Kubernetes, etc.) can wrap the binary, but their configuration is out of scope here.

## Related References
- `/ARCHITECTURE.md` — semantic source of truth.
- `../specs/runtime-config-fsm.md` — reload FSM and LKG semantics.
- `../api/runtime-admin.md` — admin/metrics endpoints used in the experiments above.
