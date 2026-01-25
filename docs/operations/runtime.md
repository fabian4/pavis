# Runtime Operations Guide

> **Class:** OPERATIONS  
> **Question:** How do I run, monitor, and operate the Runtime?  
> **Authority:** Operational procedures only. Semantic guarantees are defined in specifications and architecture.  
> **References:** See `../api/runtime-admin.md` for API semantics. See `/ARCHITECTURE.md` for invariants.

---

## Installation

### From Source
```bash
cargo build --release --bin pavis
sudo cp target/release/pavis /usr/local/bin/
```

### Container Image
```bash
docker build -t pavis:latest .
docker run -v /path/to/config.pvs:/config.pvs pavis:latest \
  --config /config.pvs
```

---

## Starting the Runtime

**Direct Execution:**
```bash
pavis --config /etc/pavis/config.pvs
```

**With Remote Relay:**
```bash
pavis --config /etc/pavis/config.pvs \
  --relay-url http://pavis-relay:8080
```

**Environment Variables:**
- `RUST_LOG`: Logging level (`info`, `debug`, `trace`)
  - Example: `RUST_LOG=pavis=debug,pavis_core=trace`
- `MALLOC_CONF`: Jemalloc tuning (non-MSVC builds only).
  - Example: `MALLOC_CONF=background_thread:true,dirty_decay_ms:1000,muzzy_decay_ms:1000`

---

## Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: pavis-proxy
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: pavis
        image: pavis:latest
        args:
          - --config
          - /config/config.pvs
          - --relay-url
          - http://pavis-relay:8080
        ports:
          - containerPort: 8080
            name: http
          - containerPort: 9901
            name: admin
        livenessProbe:
          httpGet:
            path: /health
            port: 9901
        readinessProbe:
          httpGet:
            path: /health
            port: 9901
        volumeMounts:
          - name: config
            mountPath: /config
      volumes:
        - name: config
          configMap:
            name: pavis-config
```

---

## Configuration Updates

### Hot Reload (Manual)
```bash
# Generate new artifact
pavctl compile config.yaml -o config.pvs

# Trigger reload (send SIGHUP)
kill -HUP $(pgrep pavis)
```

### Hot Reload (Automatic via Relay)
```bash
# Publish to relay
pavctl publish --relay http://relay:8080 config.pvs

# All connected runtimes automatically update within seconds
```

---

## Monitoring

### Health Checks
```bash
curl http://localhost:9901/health
curl http://localhost:9901/stats | jq
```

### Metrics
```bash
curl http://localhost:9902/metrics
```

**Key Metrics:**
- `pavis_requests_total{method,route_pattern,status}`
- `pavis_request_duration_seconds{route_pattern}`
- `pavis_connections_active`
- `pavis_upstream_requests_total{upstream,status}`

### Logs
```bash
# Access logs (JSON)
tail -f /var/log/pavis/access.log | jq

# Application logs
journalctl -u pavis -f
```

---

## Graceful Shutdown

The runtime supports graceful shutdown on `SIGTERM`:

**Process:**
1. Stop accepting new connections
2. Wait for active requests to complete (default: 30s timeout)
3. Close listeners
4. Exit cleanly

**Configuration:**
```yaml
server:
  graceful_shutdown_timeout_seconds: 60
```

**Kubernetes:**
```yaml
lifecycle:
  preStop:
    exec:
      command: ["/bin/sh", "-c", "sleep 5"]
terminationGracePeriodSeconds: 60
```

---

## Related Documents

- **API Specification**: See `../api/runtime-admin.md`
- **Recovery Procedures**: See `recovery.md`
