# Pavis Relay Operations Guide

This document provides operational procedures for running and maintaining `pavis-relay`.

## Table of Contents

- [Installation](#installation)
- [Configuration](#configuration)
- [Starting the Relay](#starting-the-relay)
- [Monitoring](#monitoring)
- [Backup and Restore](#backup-and-restore)
- [Troubleshooting](#troubleshooting)
- [Maintenance](#maintenance)

---

## Installation

### From Source

```bash
# Build the relay binary
cargo build --release --bin pavis-relay

# Install to /usr/local/bin
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
ExecStart=/usr/local/bin/pavis-relay --config /etc/pavis/relay.yaml --data-dir /var/lib/pavis-relay
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

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

### Configuration File

Default location: `relay.yaml`

```yaml
http:
  bind: "0.0.0.0:8080"

storage:
  root_dir: "/var/lib/pavis-relay"

artifact:
  lkg_path: "lkg/config.pvs"  # Relative to root_dir
  limits:
    max_pvs_bytes: 10485760   # 10 MB

pipeline:
  enabled: false
  # Pipeline config for dynamic ingestion (optional)
```

### Data Directory

Default: `/var/lib/pavis-relay` (override with `--data-dir`)

**Directory Structure:**

```
/var/lib/pavis-relay/
├── state.json              # Version cache (derived from LKG)
├── lkg/                    # Last Known Good directory
│   ├── config.pvs          # Current artifact
│   └── meta.json           # LKG metadata (AUTHORITATIVE)
└── history/                # Historical artifacts
    ├── 0000000001.pvs
    ├── 0000000001.meta.json
    ├── 0000000002.pvs
    ├── 0000000002.meta.json
    └── ...
```

### Permissions

```bash
# Create pavis user
sudo useradd -r -s /bin/false pavis

# Set up data directory
sudo mkdir -p /var/lib/pavis-relay
sudo chown pavis:pavis /var/lib/pavis-relay
sudo chmod 755 /var/lib/pavis-relay
```

---

## Starting the Relay

### Command Line

```bash
pavis-relay --config relay.yaml --data-dir /var/lib/pavis-relay
```

**CLI Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--config` | `relay.yaml` | Path to configuration file |
| `--data-dir` | `/var/lib/pavis-relay` | Data storage directory |

### Environment Variables

Currently not supported. Use configuration file.

### Startup Sequence

1. Load configuration from `relay.yaml`
2. Resolve data directory (CLI flag → config → default)
3. Create storage directories (`lkg/`, `history/`)
4. **Run crash recovery** (`repair_lkg()`)
5. Load LKG metadata (or default to version 0)
6. Verify/rewrite `state.json` if stale
7. Scan for orphaned/corrupt history entries (log warnings)
8. Start HTTP server

### Verifying Startup

```bash
# Check systemd status
sudo systemctl status pavis-relay

# Check logs
sudo journalctl -u pavis-relay -f

# Health check
curl http://localhost:8080/health

# Status check
curl http://localhost:8080/v1/status | jq .
```

**Expected status output:**

```json
{
  "status": "healthy",
  "uptime_s": 120,
  "current_version": 0,
  "lkg": null,
  "history_count": 0
}
```

---

## Monitoring

### Health Endpoints

**Liveness Probe:**
```bash
curl -f http://localhost:8080/health || echo "Relay is down"
```

**Readiness Probe:**
```bash
curl -f http://localhost:8080/ready || echo "No config published yet"
```

**Status Endpoint:**
```bash
curl http://localhost:8080/v1/status | jq .
```

### Prometheus Metrics

Scrape `/v1/metrics` for Prometheus:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'pavis-relay'
    static_configs:
      - targets: ['relay:8080']
    metrics_path: '/v1/metrics'
```

**Key Metrics:**

- `pavis_relay_version` - Current version (should increment on publish)
- `pavis_relay_publish_ok_total` - Successful publishes (increasing is good)
- `pavis_relay_publish_fail_total` - Failed publishes (investigate if increasing)
- `pavis_relay_longpoll_wait_total` - Long-poll requests (traffic indicator)

### Alerts

**Prometheus Alert Rules:**

```yaml
groups:
  - name: pavis_relay
    rules:
      - alert: RelayPublishFailures
        expr: increase(pavis_relay_publish_fail_total[5m]) > 3
        for: 1m
        annotations:
          summary: "Multiple relay publish failures detected"

      - alert: RelayDown
        expr: up{job="pavis-relay"} == 0
        for: 1m
        annotations:
          summary: "Pavis relay is down"
```

### Logs

**View logs:**

```bash
# Systemd journal
sudo journalctl -u pavis-relay -f

# Last 100 lines
sudo journalctl -u pavis-relay -n 100

# Errors only
sudo journalctl -u pavis-relay -p err
```

**Important log messages:**

| Message | Meaning | Action |
|---------|---------|--------|
| `history entry version X exceeds LKG version` | Orphaned history entry | Normal after crash, safe to ignore or manually cleanup |
| `history entry version X is missing .pvs or .meta.json` | Corrupt history entry | Investigate and manually cleanup |
| `LKG metadata size X does not match artifact size Y` | Metadata inconsistency | Warning only, verify manually |
| `Failed to persist state.json after publish` | state.json write failure | Will repair on next startup, non-critical |

---

## Backup and Restore

### What to Back Up

**Critical (MUST backup):**
- `/var/lib/pavis-relay/lkg/` - Last Known Good config
- `/var/lib/pavis-relay/history/` - Historical versions

**Optional (can be regenerated):**
- `/var/lib/pavis-relay/state.json` - Cache only

### Backup Procedure

```bash
#!/bin/bash
# backup-relay.sh

DATA_DIR="/var/lib/pavis-relay"
BACKUP_DIR="/backup/pavis-relay"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Stop relay (optional, for consistency)
sudo systemctl stop pavis-relay

# Backup LKG and history
tar -czf "$BACKUP_DIR/relay-$TIMESTAMP.tar.gz" \
  -C "$DATA_DIR" \
  lkg/ history/ state.json

# Restart relay
sudo systemctl start pavis-relay

echo "Backup completed: $BACKUP_DIR/relay-$TIMESTAMP.tar.gz"
```

### Restore Procedure

```bash
#!/bin/bash
# restore-relay.sh

BACKUP_FILE="$1"
DATA_DIR="/var/lib/pavis-relay"

if [ -z "$BACKUP_FILE" ]; then
  echo "Usage: $0 <backup-file.tar.gz>"
  exit 1
fi

# Stop relay
sudo systemctl stop pavis-relay

# Clear existing data
sudo rm -rf "$DATA_DIR/lkg" "$DATA_DIR/history" "$DATA_DIR/state.json"

# Extract backup
sudo tar -xzf "$BACKUP_FILE" -C "$DATA_DIR"

# Fix permissions
sudo chown -R pavis:pavis "$DATA_DIR"

# Start relay (will verify state.json and repair if needed)
sudo systemctl start pavis-relay

echo "Restore completed"
```

### Disaster Recovery

If all backups are lost but clients still have LKG cached:

1. Stop relay
2. Clear `/var/lib/pavis-relay`
3. Start relay (will start at version 0)
4. **Re-publish config** from source (generates new version 1)
5. Clients will detect checksum change and update

---

## Troubleshooting

### Relay Won't Start

**Symptom:** `systemctl start pavis-relay` fails

**Check:**

```bash
# View error logs
sudo journalctl -u pavis-relay -n 50

# Check config syntax
pavis-relay --config /etc/pavis/relay.yaml --help

# Check permissions
ls -la /var/lib/pavis-relay
```

**Common causes:**

- Invalid `relay.yaml` syntax
- Missing data directory
- Permission denied
- Port already in use (check `bind` address)

### Publish Fails

**Symptom:** `pavctl publish` returns error

**Check relay logs:**

```bash
sudo journalctl -u pavis-relay -f
```

**Common errors:**

| Error | Cause | Solution |
|-------|-------|----------|
| `verification failed` | Invalid PVS file | Re-generate with `pavctl gen` |
| `exceeds max_pvs_bytes` | Artifact too large | Increase `max_pvs_bytes` or reduce config size |
| `storage error` | Disk full / permissions | Check disk space and permissions |

### Clients Not Updating

**Symptom:** Runtime agents not applying new config

**Debug checklist:**

1. **Verify publish succeeded:**
   ```bash
   curl http://relay:8080/v1/status | jq .current_version
   ```

2. **Check client logs for checksum:**
   ```bash
   # Should see "config checksum unchanged" or "Applied configuration update"
   ```

3. **Manually verify checksum:**
   ```bash
   curl -i http://relay:8080/v1/config | grep -i x-config-checksum
   ```

4. **Check long-poll timeout:**
   - Clients should use `?timeout=30`
   - Too short → may miss publish events

### Orphaned History Entries

**Symptom:** Warnings on startup about orphaned versions

**Example:**
```
WARN history entry version 5 exceeds LKG version
```

**Cause:** Crash occurred after writing history but before promoting to LKG

**Resolution:**

Option 1: **Ignore** (safe, no functional impact)

Option 2: **Manual cleanup:**
```bash
# List orphaned entries
cd /var/lib/pavis-relay/history
CURRENT_VERSION=$(jq .version ../lkg/meta.json)
ls *.pvs | awk -F. '{print $1}' | while read v; do
  if [ "$v" -gt "$CURRENT_VERSION" ]; then
    echo "Orphan: $v"
    rm -v "${v}.pvs" "${v}.meta.json"
  fi
done
```

### Corrupt History Entries

**Symptom:** Warnings about missing `.pvs` or `.meta.json`

**Example:**
```
WARN history entry version 3 is missing .pvs or .meta.json
```

**Resolution:**

```bash
# Find corrupt entries
cd /var/lib/pavis-relay/history
for pvs in *.pvs; do
  version=$(basename "$pvs" .pvs)
  meta="${version}.meta.json"
  if [ ! -f "$meta" ]; then
    echo "Missing metadata for $pvs"
    rm -v "$pvs"
  fi
done

for meta in *.meta.json; do
  version=$(basename "$meta" .meta.json)
  pvs="${version}.pvs"
  if [ ! -f "$pvs" ]; then
    echo "Missing artifact for $meta"
    rm -v "$meta"
  fi
done
```

---

## Maintenance

### Inspecting Current Version

```bash
# Via API
curl http://relay:8080/v1/status | jq .current_version

# Via filesystem
jq .version /var/lib/pavis-relay/lkg/meta.json
```

### Inspecting LKG Checksum

```bash
jq .checksum /var/lib/pavis-relay/lkg/meta.json
```

**Manually verify:**
```bash
STORED=$(jq -r .checksum /var/lib/pavis-relay/lkg/meta.json)
COMPUTED=$(sha256sum /var/lib/pavis-relay/lkg/config.pvs | awk '{print "sha256:"$1}')

if [ "$STORED" = "$COMPUTED" ]; then
  echo "Checksum verified: $STORED"
else
  echo "ERROR: Checksum mismatch!"
  echo "Stored:   $STORED"
  echo "Computed: $COMPUTED"
fi
```

### History Cleanup (Manual)

**View history:**

```bash
ls -lh /var/lib/pavis-relay/history/
```

**Calculate history size:**

```bash
du -sh /var/lib/pavis-relay/history/
```

**Delete old versions (keep last N):**

```bash
#!/bin/bash
# cleanup-history.sh

HISTORY_DIR="/var/lib/pavis-relay/history"
KEEP_COUNT=10

# Get all version numbers, sorted
VERSIONS=$(ls "$HISTORY_DIR"/*.pvs 2>/dev/null | \
  sed 's/.*\/\([0-9]*\)\.pvs/\1/' | \
  sort -n)

TOTAL=$(echo "$VERSIONS" | wc -l)
DELETE_COUNT=$((TOTAL - KEEP_COUNT))

if [ "$DELETE_COUNT" -le 0 ]; then
  echo "No cleanup needed (only $TOTAL versions)"
  exit 0
fi

echo "Deleting oldest $DELETE_COUNT versions..."

echo "$VERSIONS" | head -n "$DELETE_COUNT" | while read version; do
  rm -v "$HISTORY_DIR/${version}.pvs" "$HISTORY_DIR/${version}.meta.json"
done

echo "Cleanup complete"
```

**CAUTION:** Do NOT delete LKG version from history!

### Rotating Logs

**Systemd journal** auto-rotates. Configure in `/etc/systemd/journald.conf`:

```ini
[Journal]
SystemMaxUse=1G
SystemMaxFileSize=100M
```

Apply:
```bash
sudo systemctl restart systemd-journald
```

### Upgrading Relay

```bash
# Stop relay
sudo systemctl stop pavis-relay

# Backup data
./backup-relay.sh

# Install new binary
sudo cp target/release/pavis-relay /usr/local/bin/

# Start relay (will auto-repair if needed)
sudo systemctl start pavis-relay

# Verify
curl http://localhost:8080/v1/status
```

---

## Performance Tuning

### File Descriptors

For high-traffic deployments:

```bash
# /etc/security/limits.conf
pavis soft nofile 65536
pavis hard nofile 65536
```

### Filesystem

**Recommended:**
- ext4 or xfs (for fsync performance)
- SSD preferred (frequent small writes)
- Dedicated partition for `/var/lib/pavis-relay`

**Mount options:**
```
/dev/sdb1  /var/lib/pavis-relay  ext4  defaults,noatime  0  2
```

### Max PVS Size

Increase limit if needed:

```yaml
artifact:
  limits:
    max_pvs_bytes: 52428800  # 50 MB
```

**Trade-offs:**
- Larger → more memory per request
- Larger → longer SHA256 computation
- Larger → slower network transfer

---

## Security Best Practices

1. **Network isolation:** Deploy in trusted network
2. **Firewall:** Restrict access to relay port
3. **No public exposure:** Use reverse proxy with auth
4. **Monitor metrics:** Alert on unusual publish rates
5. **Backup encryption:** Encrypt backup archives
6. **Principle of least privilege:** Run as dedicated user

---

## Common Tasks

### Resetting Relay to Version 0

```bash
sudo systemctl stop pavis-relay
sudo rm -rf /var/lib/pavis-relay/*
sudo systemctl start pavis-relay
```

### Inspecting Artifact Content

```bash
# View current LKG
pavis-pvs inspect /var/lib/pavis-relay/lkg/config.pvs

# View historical version
pavis-pvs inspect /var/lib/pavis-relay/history/0000000001.pvs
```

### Comparing Versions

```bash
# Extract checksums
jq .checksum /var/lib/pavis-relay/history/0000000001.meta.json
jq .checksum /var/lib/pavis-relay/history/0000000002.meta.json

# If checksums match → identical configs (different versions)
```

---

## See Also

- [API Reference](../api/relay.md)
- [Crash Recovery Guide](crash-recovery.md)
- [Architecture Documentation](../../ARCHITECTURE.md)
