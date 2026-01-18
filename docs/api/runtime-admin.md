# Runtime Admin API

Pavis provides a **read-only** admin API for health checks and runtime introspection.

**Configuration:**

```yaml
admin:
  enabled: false              # Default: false (disabled)
  address: "127.0.0.1:9901"   # Default: loopback only
```

**Security Note:** The admin API has **no authentication** in the current release. Bind to loopback (`127.0.0.1`) or use firewall rules to restrict access.

## Endpoints

| Endpoint | Description | Response |
|----------|-------------|----------|
| `GET /health` | Health status | `{"status":"healthy"}` (always 200 OK) |
| `GET /stats` | Runtime statistics | JSON with version, uptime, config counts |

### Stats Response Example

```json
{
  "version": "0.0.0",
  "uptime_seconds": 3600,
  "listeners": 2,
  "upstreams": 5,
  "routes": 10
}
```

## Kubernetes Integration

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 9901
  initialDelaySeconds: 5
  periodSeconds: 10
```
