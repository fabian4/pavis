# Pavis E2E Tests

## 1. Crate Overview
`pavis-e2e` contains the end-to-end testing suite for the Pavis system. It is responsible for verifying the integrated behavior of the entire stack, including the `pavis` runtime, the `pavis-relay` control plane, and their interactions with upstream backend services.

Its primary responsibilities are:
- Providing a test harness (`pavis-e2e::support`) to spin up ephemeral environments containing proxies, relays, and mock upstreams.
- Executing black-box tests against running binaries to verify routing logic, protocol compliance, and operational resilience.
- Simulating network scenarios, such as upstream failures, latency, and partitioning (in integrated tests).
- Validating the configuration pipeline from high-level YAML -> Relay -> Agent -> Runtime.

It explicitly does not handle:
- Unit testing of individual crates (those are within `crates/*/src` or `crates/*/tests`).
- Codec correctness in isolation (handled by `pavis-codec-*` tests).

## 2. Features
- **Ephemeral Environments**: The `TestEnv` and `RelayEnv` helpers manage the lifecycle of child processes, temporary directories, and network ports, ensuring hermetic test execution.
- **Mock Upstreams**: Uses `axum` to create lightweight, controllable backend servers (`UpstreamEnv`) that can assert received headers and inject specific responses.
- **Binary Discovery**: Automatically locates debug or release binaries for `pavis` and `pavis-relay` within the cargo workspace.
- **Docker Integration**: Includes support for resolving services in Docker Compose environments for complex integrated scenarios (via `resolve_docker_service_ip`).

## 3. Module Breakdown

### `support`
The core test infrastructure library.
- `pavis`: Helpers for spawning the `pavis` binary, generating configurations, and waiting for readiness.
- `relay`: Infrastructure for running `pavis-relay` and its dependencies (mock ingest sources).
- `upstream`: A configurable HTTP backend server for asserting request reception and properties.
- `scenario`: Logic to orchestrate multi-service tests.

### `tests/pavis`
Standalone runtime tests. These verify the data plane in isolation.
- **Routing**: Regex matching, prefix/exact precedence, weighted splitting, wildcard hosts.
- **Traffic**: Round-robin balancing, header manipulation, TLS termination, upstream TLS.
- **Lifecycle**: Startup failures, config validation, signal handling.

### `tests/relay`
Control plane tests. These verify the relay's serving logic.
- **API**: HTTP API contract, version negotiation, long-polling behavior.

### `tests/integrated`
System-wide integration tests. These verify the interaction between components.
- **Pipeline**: Publishing a config to Relay and observing it apply in Pavis.
- **Resilience**: Recovery from relay unavailability, partition tolerance.
- **Concurrency**: Behavior under load (concurrent config pushes).

## 4. Public API Surface (Test Harness)

This crate is not a library for external consumption, but it exposes internal `pub` modules for use by its own integration tests.

### `TestEnv`
Manages a `pavis` child process.
- `start(config: &RuntimeConfig)`: Writes the config to disk and starts the process.
- `client()`: Returns a `reqwest::Client` configured to talk to the proxy.
- `stop()`: Gracefully terminates the process.

### `UpstreamSet`
Manages a collection of backend servers.
- `spawn(n: usize)`: Creates `n` listeners on random ports.
- `addresses()`: Returns the list of `SocketAddr` for the upstreams.

## 5. Configuration and Runtime Behavior

### Test Execution
Tests are run via `cargo test -p pavis-e2e`. They rely on `cargo build` having been run previously to produce the necessary binaries.

### Environment
- **Ports**: Tests bind to port 0 to let the OS assign free ephemeral ports, allowing high concurrency.
- **Temp Dirs**: Each test gets a unique temporary directory for configuration files and logs, cleaned up on drop.

## 6. Error Handling and Invariants

### Timeout Safety
Most assertions wait for a condition (like "proxy ready" or "config applied") with a timeout. This prevents tests from hanging indefinitely on failure.

### Cleanup
The `Drop` implementations for `TestEnv` and `RelayEnv` ensure that child processes are killed even if a test panics, preventing orphan processes.

## 7. Non-Goals and Explicit Limitations
- **Performance Testing**: These tests target correctness, not throughput or latency (see `bench/` for performance).
- **Chaos Engineering**: While it simulates some failures, it is not a full chaos engineering suite like Chaos Mesh.