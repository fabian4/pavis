# Runtime Crash Recovery Guide

> **Class:** OPERATIONS  
> **Question:** How do I recover from runtime crashes and failures?  
> **Authority:** Operational procedures only. Semantic guarantees are defined in specifications and architecture.  
> **References:** See `/ARCHITECTURE.md` for invariants. See `runtime.md` for normal operations.

---

## Automatic Recovery

The runtime is designed to restart cleanly after crashes:

**State Reconstruction:**
1. Reload `.pvs` configuration from disk
2. Recompile router and upstream manager
3. Re-bind listeners
4. Resume serving traffic

**No State Loss:** All state is derived from `.pvs` artifact (stateless proxy).

---

## Common Failure Modes

### Problem: Runtime exits with "failed to load config"

**Cause:** Invalid `.pvs` file or version mismatch

**Recovery:**
```bash
# Verify artifact
pavis-pvs verify /etc/pavis/config.pvs

# Regenerate if corrupt
pavctl compile config.yaml -o /etc/pavis/config.pvs

# Restart runtime
systemctl restart pavis
```

### Problem: Runtime exits with "failed to bind listener"

**Cause:** Port already in use or permission denied

**Recovery:**
```bash
# Check port usage
sudo lsof -i :8080

# Fix permissions (if binding to port <1024)
sudo setcap 'cap_net_bind_service=+ep' /usr/local/bin/pavis

# Or run as root (not recommended)
```

### Problem: High memory usage / OOM kills

**Cause:** Too many concurrent connections or large payload buffering

**Recovery:**
```bash
# Set memory limits in systemd
echo "MemoryMax=2G" >> /etc/systemd/system/pavis.service
systemctl daemon-reload
systemctl restart pavis

# Or Kubernetes:
resources:
  limits:
    memory: 2Gi
```

### Problem: Connections hang / timeout

**Cause:** Upstream unavailable or DNS resolution failure

**Recovery:**
```bash
# Check upstream health
curl -v http://upstream-service:8080/health

# Check DNS
nslookup upstream-service

# Enable debug logging
RUST_LOG=pavis=debug pavis --config config.pvs
```

---

## Systematic Recovery Procedure

### Step 1: Identify Failure
```bash
# Check exit code
echo $?

# Check systemd status
systemctl status pavis

# Check logs
journalctl -u pavis -n 100 --no-pager
```

### Step 2: Categorize Issue

| Exit Code | Meaning | Action |
|-----------|---------|--------|
| 0 | Clean shutdown | Normal |
| 1 | Configuration error | Verify `.pvs` file |
| 101 | Panic | Check logs for stack trace |
| 137 | SIGKILL (OOM) | Increase memory limit |
| 143 | SIGTERM | Graceful shutdown (normal) |

### Step 3: Apply Fix

**Configuration Errors:**
```bash
# Validate artifact
pavis-pvs verify config.pvs

# Check version compatibility
pavis --version
```

**Runtime Panics:**
```bash
# Extract panic info from logs
journalctl -u pavis | grep "panicked at"

# Common panics:
# - Regex compilation failure → fix regex in source config
# - Upstream connection pool exhausted → increase limits
```

**Resource Exhaustion:**
```bash
# Check system resources
free -h
df -h

# Check file descriptors
lsof -p $(pgrep pavis) | wc -l
ulimit -n

# Increase limits
echo "LimitNOFILE=65536" >> /etc/systemd/system/pavis.service
```

### Step 4: Restart
```bash
systemctl restart pavis

# Verify startup
curl http://localhost:9901/health
curl http://localhost:9901/stats
```

---

## Disaster Recovery

### Complete Data Plane Failure

**1. Identify Last Known Good Config:**
```bash
# Check relay for latest version
curl http://relay:8080/v1/status | jq '.current_version'
```

**2. Fetch and Deploy:**
```bash
curl http://relay:8080/v1/config -o /etc/pavis/config.pvs
pavis-pvs verify /etc/pavis/config.pvs
systemctl restart pavis
```

**3. Verify Traffic Flow:**
```bash
curl -v http://localhost:8080/health
```

### Rollback to Previous Version

```bash
# Fetch historical version from relay
curl http://relay:8080/v1/artifacts/42 -o /etc/pavis/config.pvs
systemctl restart pavis
```

---

## Related Documents

- **Operations Guide**: See `runtime.md` for normal operations
- **API Specification**: See `../api/runtime-admin.md`
