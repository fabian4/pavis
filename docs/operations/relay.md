# Relay Operations Guide

> **Class:** OPERATIONS  
> **Question:** How do I run, monitor, and troubleshoot the Relay?  
> **Authority:** Operational procedures only. Semantic guarantees are defined in specifications and architecture.  
> **References:** See `../specs/relay-protocol.md` and `../api/relay.md` for protocol semantics. See `/ARCHITECTURE.md` for invariants.

---

## Installation

### From Source
```bash
cargo build --release --bin pavis-relay
sudo cp target/release/pavis-relay /usr/local/bin/
```

### Systemd Service

Create `/etc/systemd/system/pavis-relay.service`:
```ini
[Unit]
Description=Pavis Configuration Relay
After=network.target

[Service]
Type=simple
User=pavis
Group=pavis
ExecStart=/usr/local/bin/pavis-relay \
  --config /etc/pavis/relay.yaml \
  --data-dir /var/lib/pavis-relay
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable pavis-relay
sudo systemctl start pavis-relay
```

---

## Configuration

**Minimal Configuration (`relay.yaml`):**
```yaml
http:
  bind: "0.0.0.0:8080"

storage:
  root_dir: "/var/lib/pavis-relay"
```

**Data Directory Structure:**
```
/var/lib/pavis-relay/
├── lkg/
│   ├── config.pvs
│   └── meta.json
└── history/
    ├── 0000000001.pvs
    └── 0000000001.meta.json
```

---

## Starting the Relay

```bash
pavis-relay --config relay.yaml --data-dir /var/lib/pavis-relay
```

**Verify Startup:**
```bash
# Health check
curl http://localhost:8080/health

# Status
curl http://localhost:8080/v1/status
```

---

## Publishing Configuration

**Using curl:**
```bash
curl -X POST http://localhost:8080/v1/publish \
  --data-binary @config.pvs \
  -H "Content-Type: application/octet-stream"
```

**Using pavctl:**
```bash
pavctl publish --relay http://localhost:8080 config.pvs
```

---

## Monitoring

### Health Checks
```bash
curl -f http://localhost:8080/health || exit 1
```

### Metrics
```bash
curl http://localhost:8080/metrics
```

**Key Metrics:**
- `pavis_relay_version`: Monitor for config changes
- `pavis_relay_publish_ok_total`: Track successful publish rate
- `pavis_relay_publish_fail_total`: Track failed publishes
- `pavis_relay_longpoll_wait_total`: Long-poll waits (counter)

### Logs
```bash
journalctl -u pavis-relay -f
```

**Look for:**
- "Published version X" (successful publishes)
- "Verified PVS artifact" (validation succeeded)
- "LKG persisted" (disk writes succeeded)

---

## Backup and Restore

### Backup LKG
```bash
cp /var/lib/pavis-relay/lkg/config.pvs backup-$(date +%F).pvs
cp /var/lib/pavis-relay/lkg/meta.json backup-$(date +%F).json
```

### Restore from Backup
```bash
systemctl stop pavis-relay
cp backup.pvs /var/lib/pavis-relay/lkg/config.pvs
cp backup.json /var/lib/pavis-relay/lkg/meta.json
systemctl start pavis-relay
```

---

## Troubleshooting

### Problem: Relay won't start

**Check logs:**
```bash
journalctl -u pavis-relay -n 50
```

**Common causes:**
- Port 8080 in use: `sudo lsof -i :8080`
- Data directory permissions: `sudo chown -R pavis:pavis /var/lib/pavis-relay`
- Corrupt LKG file: Restore from backup

### Problem: Publish fails with 400

**Verify PVS artifact:**
```bash
pavis-pvs verify config.pvs
```

**Check relay logs:**
```bash
journalctl -u pavis-relay | grep "verification failed"
```

### Problem: Clients not receiving updates

**Check relay version:**
```bash
curl http://localhost:8080/v1/status | jq '.current_version'
```

**Verify long-poll:**
```bash
ETAG=$(curl -sS -D - http://localhost:8080/v1/config -o /dev/null \
  | awk 'tolower($1)=="etag:" {print $2}' | tr -d '\r')
curl -v http://localhost:8080/v1/config?wait_ms=5000 \
  -H "If-None-Match: $ETAG"
# Should block for 5 seconds then return 204
```

---

## Related Documents

- **API Specification**: See `../api/relay.md`
- **Protocol Specification**: See `../specs/relay-protocol.md`
