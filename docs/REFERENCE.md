# Reference

> **Role:** Canonical Developer Reference for APIs and Configuration.

## 1. Runtime Configuration

This section describes the `pavis_core::RuntimeConfig` structure consumed by the Pavis runtime.

### 1.1 Normative Semantics
- **Headers**: `HeadersPolicy::Disabled` means no header mutations.
- **Regex**: Compilation happens at runtime load and is not stored in the schema.
- **Durations**: YAML durations (`idle`, `connect`) accept strings (e.g., "5s") but are materialized into `u32` milliseconds in the `.pvs` binary.

### 1.2 Configuration Fields (YAML Layout)

The canonical Rust schema lives in `crates/pavis-core/src/runtime/`.

```yaml
# Canonical YAML Reference Template
# Corresponds to `RuntimeConfig` in pavis-core

server:
  # Binding address
  listen: "0.0.0.0:8080"
  
  # TLS configuration (Optional)
  tls:
    mode: "disabled" # or "enabled"
    # required if enabled:
    # cert_path: "/path/to/cert.pem"
    # key_path: "/path/to/key.pem"

telemetry:
  access_log:
    enabled: true
    path: "stdout" # or file path

upstreams:
  - name: "backend-a"
    load_balancer: "round_robin"
    endpoints:
      - address: "127.0.0.1:8081"
        weight: 1

routes:
  - match:
      path: "/api/v1"
      type: "prefix"
    destination:
      upstream: "backend-a"
```

---

## 2. Relay HTTP API

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

Operational status and health. Returns internal state (name, active version, checksum, uptime).
