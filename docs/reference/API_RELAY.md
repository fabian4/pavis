# Relay HTTP API Reference

> **Status:** Reference
> **Role:** The canonical definition of the Pavis Relay HTTP API.

## Endpoints

### `GET /v1/config`

Fetches the latest configuration artifact. Supports Long-Polling.

**Request Headers:**

| Header | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `X-Pavis-Artifact-Version` | `u64` | Yes | The version currently held by the client. |

**Query Parameters:**

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `wait_ms` | `u64` | `1000` | Max time to hold connection if up-to-date (Max 10s). |

**Responses:**

| Status | Description |
| :--- | :--- |
| `200 OK` | New configuration available. Body is `.pvs` binary. |
| `204 No Content` | Timeout reached, no new config. Client should retry. |
| `400 Bad Request` | Missing or invalid headers/params. |

### `GET /v1/status`

Operational status and health.

**Response Body (Plain Text or JSON):**

Returns internal state (name, active version, checksum, uptime).

## Protocol Details

See [docs/specs/RELAY_PROTOCOL.md](../specs/RELAY_PROTOCOL.md) for the server-side state machine and long-polling logic.
