# E2E Testing Overview

## Testing Strategy

Pavis employs a standard testing pyramid with strict coverage rules.

### The Testing Pyramid

1. **Unit Tests (Rust)**: High coverage. Focus on logic, codecs, and core validation.
   - Command: `cargo test`
2. **Integration Tests**: Focus on component interaction (Codec -> Core, Ingest -> Relay).
   - Command: `cargo test --test integrated`
3. **End-to-End (E2E) Tests**: Black-box testing of compiled binaries.
   - Command: `make test-e2e`

### Testing Rules

- **Core Guard**: `pavis-core` MUST NOT depend on test helpers from upper layers.
- **Mocking**: Use traits for IO boundaries to enable unit testing of logic without network.
- **Symlink Tests**: Ingest tests MUST verify symlink following (Kubernetes ConfigMap behavior).

---

## E2E Test Organization

E2E tests are organized into three categories:

### 1. Integrated Tests (Relay + Pavis)
See: [integrated_cases.md](./integrated_cases.md)

Tests the full control plane + data plane integration:
- Configuration distribution via relay
- Long-polling and version updates
- Network partitions and recovery
- Security features (TLS, mTLS, RBAC)

**Run with**: `make e2e-integrated-binary` or `make e2e-integrated`

### 2. Pavis-Only Tests (Runtime)
See: [pavis_cases.md](./pavis_cases.md)

Tests the data plane runtime in isolation:
- PVS file validation and loading
- Routing logic (prefix, exact, regex matching)
- TLS termination
- Load balancing and traffic splitting
- Header manipulation
- DNS discovery

**Run with**: `make e2e-pavis-binary` or `make e2e-pavis`

### 3. Relay-Only Tests (Control Plane)
See: [relay_cases.md](./relay_cases.md)

Tests the control plane in isolation:
- Artifact versioning and publishing
- Long-poll semantics
- File watching and ingestion
- Crash recovery and persistence
- Configuration validation

**Run with**: `make e2e-relay`

---

## Test Execution Modes

### Binary Mode (Default)
- Uses locally compiled binaries from `target/release/`
- Backend services run in Docker containers
- Faster iteration for development
- Best for debugging with logs and debuggers

```bash
TEST_MODE=binary make e2e-integrated
```

### Docker Mode
- All components run in Docker containers
- More production-like environment
- Better isolation and reproducibility
- Slower startup but more realistic

```bash
TEST_MODE=docker make e2e-integrated
```

---

## Running Tests

### Run All E2E Tests
```bash
make test-e2e
```

### Run Specific Test Suites
```bash
make e2e-integrated-binary  # Integrated tests (binary mode)
make e2e-pavis-binary       # Pavis-only tests (binary mode)
make e2e-relay              # Relay-only tests
```

### Run Individual Test Cases
```bash
# Run specific test by name
cargo test --package pavis-e2e --test integrated integrated_tls_propagation

# Run with verbose output
cargo test --package pavis-e2e --test integrated -- --nocapture
```

---

## Test Case Naming Convention

- **I{N}**: Integrated tests (Relay + Pavis)
- **P{N}**: Pavis-only tests (Runtime)
- **R{N}**: Relay-only tests (Control Plane)

Tests are numbered sequentially and documented with:
- Setup: Initial conditions
- Action: Operations performed
- Expect: Expected outcomes
- Rationale: Why this test matters
