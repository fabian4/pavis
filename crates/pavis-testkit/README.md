# Pavis Testkit

This crate provides E2E test helper binaries: `pavis-mock-upstream` and `pavis-mock-relay`.

## Usage

### pavis-mock-upstream

Mocks an upstream service with predictable behaviors.

**Run:**
```bash
cargo run -p pavis-testkit --bin pavis-mock-upstream -- --http-port 8080
```

**Endpoints:**
- `GET /healthz`: Returns 200 OK.
- `GET /echo`: Returns JSON with request details.
- `GET /delay?ms=100`: Delays response by `ms` milliseconds.
- `GET /status?code=500`: Returns specified status code.

### pavis-mock-relay

Mocks the Pavis Relay service for config distribution.

**Run:**
```bash
cargo run -p pavis-testkit --bin pavis-mock-relay -- --listen 127.0.0.1:8081
```

**Endpoints:**
- `POST /publish`: Publish a new artifact (raw body).
- `GET /v1/longpoll`: Long-poll for artifact updates.
- `GET /status`: Get current artifact metadata.

## Configuration

Both binaries support CLI flags and Environment variables. Use `--help` to see all options.
