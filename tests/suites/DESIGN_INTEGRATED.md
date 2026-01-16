# Pavis Integrated Suite: Design & Strength Review

## 1. Suite Goals
The Integrated Suite serves as the final verification layer, proving that independent components (`pavctl`, `pavis-relay`, `pavis` runtime, and `pavis-mock-upstream`) function correctly as a distributed system.

Its primary goal is to prove that a configuration compiled by a user propagtes through the control plane and is applied by the data plane without downtime.

## 2. Core Invariants
*   **I1 (End-to-End Publish):** A valid configuration compiled by `pavctl` and published to `relay` becomes active in `pavis` within a bounded time.
*   **I2 (Hot Reload Pipeline):** The runtime successfully updates its configuration via long-poll from the relay without process restarts.
*   **I3 (Artifact Opaqueness):** The relay successfully transfers artifacts regardless of content.
*   **I4 (System LKG):** If a bad update enters the relay, the runtime rejects it and maintains traffic service using the Last-Known-Good configuration.
*   **I5 (Deployment Parity):** The integration logic holds true whether components run as native binaries or Docker containers.

---

## 3. Case Design & Strength Analysis

### `10_bootstrap_path`
*   **Intent**: Full path bootstrap (pavctl -> relay -> runtime).
*   **Strength**: ✅ Solid.

### `20_reload_switch`
*   **Intent**: Dynamic route switch across full path.
*   **Strength**: ✅ Solid.

### `21_reload_stable`
*   **Intent**: Stability under redundant updates.
*   **Strength**: ✅ Solid.

### `30_lkg_artifact`
*   **Intent**: System-wide LKG preservation.
*   **Strength**: ⚠️ Needs Expansion. Relies on fixed `sleep` to assume poll happened.
*   **Expansion**: Assert version header in runtime response remains at LKG version despite higher relay version.

### `31_lkg_rejection`
*   **Intent**: Integrated semantic rejection.
*   **Status**: ⏭️ Skipped (runtime accepts listener/TLS errors lazily, so the update is applied).

### `40_resilience_restart`
*   **Intent**: Recovery after relay restart.
*   **Strength**: ✅ Solid. Recently updated to use mode-agnostic `stop_sut`.

---

## 4. Implementation Principles
*   **Runner Managed**: lifecycle managed by `run.sh`; cases use `run_pavis` / `run_relay`.
*   **Black-Box**: Assertions based on HTTP status codes and upstream `/echo` data.
*   **Isolation**: Unique `X-Pavis-Test-Run` headers per case.
