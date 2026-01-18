# Runtime Operations

This document details the operational configuration of the Pavis runtime (`pavis`).

## 1. Graceful Shutdown

Pavis supports graceful shutdown to ensure in-flight requests complete before the process exits.

**Configuration:**

```yaml
shutdown:
  enabled: true           # Default: true
  drain_timeout_ms: 30000 # Default: 30 seconds
```

**Behavior:**
- **SIGTERM/SIGINT**: Triggers graceful shutdown
- **Drain Phase**: Stops accepting new connections and waits for in-flight requests to complete (up to `drain_timeout_ms`)
- **Force Close**: After timeout expires, remaining connections are closed immediately

**Recommendations:**
- **Production**: 30s-60s drain timeout (allows slow requests to complete)
- **Development**: `enabled: false` for fast iteration
- **High-traffic**: 60s+ for long-running requests
