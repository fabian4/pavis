# Runtime Admin API

> **Class:** API  
> **Question:** How do clients interact with the Runtime Admin API?  
> **Authority:** This document is normative. Implementation resides in code (`crates/pavis`).

---

## Overview

The Pavis Runtime exposes a **read-only** admin API for health checks and runtime introspection. This API is intended for monitoring, observability, and operational tooling.

**Security:** No authentication. Bind to loopback (`127.0.0.1`) or use firewall rules to restrict access.

---

## Configuration

```yaml
admin:
  enabled: false              # Default: disabled
  address: "127.0.0.1:9901"   # Default: loopback only
```

**Security Note:** The admin API has **no authentication** in the current release. Deploy with appropriate network controls.

---

## Endpoints

### GET /health

Liveness probe for health checks.

**Response (200 OK):**
```json
{"status": "healthy"}
```

**Semantics:**
- Always returns 200 OK if runtime is running
- Suitable for Kubernetes liveness probes
- Does NOT check upstream health (only runtime process health)

**Usage:**
```yaml
# Kubernetes liveness probe
livenessProbe:
  httpGet:
    path: /health
    port: 9901
  initialDelaySeconds: 5
  periodSeconds: 10
```

---

### GET /stats

Runtime statistics and metadata.

**Response (200 OK):**
```json
{
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "listeners": 2,
  "upstreams": 5,
  "routes": 10,
  "current_connections": 42
}
```

**Response Fields:**
- `version` (string): Runtime version
- `uptime_seconds` (u64): Seconds since startup
- `listeners` (usize): Number of configured listeners
- `upstreams` (usize): Number of configured upstream clusters
- `routes` (usize): Total number of routes across all virtual hosts
- `current_connections` (usize): Active connection count

**Semantics:**
- Snapshot of current runtime state
- NOT historical data (use metrics for time-series)

---

## Telemetry Integration

The admin API complements the telemetry system:

**Admin API:**
- Liveness/readiness checks
- Runtime metadata
- Snapshot statistics

**Metrics Endpoint** (`/metrics` on separate port):
- Time-series metrics
- Prometheus format
- Request/connection/upstream metrics
- Config validation counters (`pavis_config_validation_total{result,reason}`)
- Config apply counters (`pavis_config_apply_total{result}`)

**Access Logs:**
- Per-request structured logs
- JSON format
- Configurable destination (stdout/file)

**Tracing:**
- OpenTelemetry spans
- OTLP export to collectors
- Distributed tracing

---

## Related Documents

- **Operations Guide**: See `../operations/runtime.md` for deployment and monitoring
- **Architecture**: See `/ARCHITECTURE.md` for system invariants
