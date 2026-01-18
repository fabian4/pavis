# pavis-relay

The central configuration distribution hub for the Pavis ecosystem.

## Purpose

`pavis-relay` implements the control plane for configuration management, responsible for ingesting validated `.pvs` artifacts, assigning monotonic version numbers, and distributing updates to runtime instances via HTTP long-polling.

## Responsibilities

- Accepting and validating published configuration artifacts
- Assigning strictly monotonic version numbers
- Maintaining Last Known Good (LKG) configuration on disk
- Serving configurations via long-polling HTTP API
- Notifying connected runtimes of updates

## Module Structure

- **`app`**: Axum server setup and routing
- **`state`**: In-memory state management with atomic transitions
- **`handlers`**: HTTP endpoint implementations
- **`storage`**: Filesystem persistence layer (LKG and history)
- **`config`**: Configuration schema and loading

## Public API

The relay exposes an HTTP API (see specifications for details):
- `POST /v1/publish` - Publish new configuration
- `GET /v1/config` - Fetch current configuration (with long-polling)
- `GET /v1/status` - Relay status and metadata
- `GET /health` - Liveness probe
- `GET /metrics` - Prometheus metrics

## Related Documentation

- **API Specification**: See [`docs/api/relay.md`](../../docs/api/relay.md)
- **Protocol Specification**: See [`docs/specs/relay-protocol.md`](../../docs/specs/relay-protocol.md)
- **Operations Guide**: See [`docs/operations/relay.md`](../../docs/operations/relay.md)
- **Architecture**: See [`/ARCHITECTURE.md`](../../ARCHITECTURE.md)
