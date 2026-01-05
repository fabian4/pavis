# Testing Strategy & Plans

## 1. Testing Strategy

Pavis employs a standard testing pyramid with strict coverage rules.

### 1.1 The Pyramid

1.  **Unit Tests (Rust)**: High coverage. Focus on logic, codecs, and core validation.
    *   Command: `cargo test`
2.  **Integration Tests**: Focus on component interaction (Codec -> Core, Ingest -> Relay).
    *   Command: `cargo test --test integrated`
3.  **End-to-End (E2E) Tests**: Black-box testing of compiled binaries.
    *   Command: `make test-e2e`

### 1.2 Rules
- **Core Guard**: `pavis-core` MUST NOT depend on test helpers from upper layers.
- **Mocking**: Use traits for IO boundaries to enable unit testing of logic without network.
- **Symlink Tests**: Ingest tests MUST verify symlink following (Kubernetes ConfigMap behavior).

---

## 2. Integrated E2E Cases

### I1: Publish -> Long-poll -> Runtime Apply
- Setup: Relay, Runtime, Upstreams A/B.
- Action: Publish v1 (Route -> A). Runtime polls.
- Expect: Runtime routes to A. Headers show v1.
- Action: Publish v2 (Route -> B).
- Expect: Runtime updates hot, routes to B.

### I2: Invalid Publish
- Action: Publish invalid config.
- Expect: Relay rejects OR Runtime refuses apply. LKG stays on valid version.

### I3: Concurrency
- Setup: 3 Runtimes.
- Action: Publish v1, v2, v3 rapidly.
- Expect: All runtimes converge to v3.

### I4: Pipeline (File Ingest -> Relay -> Runtime)
- Setup: Relay watching `input.yaml`.
- Action: Write valid YAML to `input.yaml`.
- Expect: Relay ingests, Runtime applies.

### I5: Data Plane Recovery
- Setup: Relay v2. Kill Runtime.
- Action: Restart Runtime.
- Expect: Runtime boots, fetches v2, restores traffic.

---

## 3. Pavis E2E Cases (Runtime-Only)

### P1: Reject Invalid PVS
- Action: Provide corrupted `.pvs`.
- Expect: Fail fast on startup with `checksum mismatch`.

### P2: Runtime Apply Semantics
- Action: Swap `.pvs` file and restart.
- Expect: Traffic switches to new config.

### P3: Missing Config Path
- Action: Start with non-existent path.
- Expect: Non-zero exit code.

---

## 4. Relay E2E Cases (Relay-Only)

### R1: Atomic LKG Update
- Action: Publish v1.
- Expect: `GET /v1/artifacts/1` works. `GET /v1/status` shows v1.

### R2: Long-Poll Semantics
- Action: Poll with `wait_ms=1000` and current version.
- Expect: Hold until timeout (304) or update (200).

### R3: Ingest Debouncing
- Action: Write file 5 times in 100ms.
- Expect: Single version increment.

### R4: Crash Safety
- Action: Kill Relay, restart.
- Expect: State recovered from disk (LKG).

### R5: Symlink Updates
- Action: Update symlink `input.yaml` -> `data/v2.yaml`.
- Expect: Watcher detects change, updates config.
