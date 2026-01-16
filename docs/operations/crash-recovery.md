# Pavis Relay Crash Recovery Guide

This document describes crash recovery procedures and invariants for `pavis-relay`.

## Overview

The relay is designed to survive crashes at any point during the publish flow. This document explains:
- How crashes are handled automatically
- Recovery invariants that must hold
- Manual recovery procedures
- Debugging corrupted state

---

## Table of Contents

- [Crash Recovery Philosophy](#crash-recovery-philosophy)
- [Publish Flow Atomicity](#publish-flow-atomicity)
- [Automatic Recovery on Startup](#automatic-recovery-on-startup)
- [Recovery Scenarios](#recovery-scenarios)
- [Manual Recovery Procedures](#manual-recovery-procedures)
- [Debugging Tools](#debugging-tools)

---

## Crash Recovery Philosophy

### Design Principles

1. **LKG Metadata is Authoritative**: The file `lkg/meta.json` is the single source of truth for current version
2. **state.json is a Cache**: Always derived from LKG, can be rewritten
3. **Atomic Writes**: All critical files use write-tmp-rename-fsync pattern
4. **Orphans are Safe**: Unpromoted history entries are harmless and can remain
5. **Fail-Safe Defaults**: Missing files default to version 0 (bootstrap state)

### Success Criterion

A publish is successful **if and only if** LKG promotion completes:
```
✅ Success = lkg/config.pvs AND lkg/meta.json both exist
❌ Failure = Either file missing or incomplete
```

Version increments **only** after successful LKG promotion.

---

## Publish Flow Atomicity

### Ordered Steps

Publish proceeds in strict order:

```
1. Validate PVS artifact        (in-memory)
2. Compute checksum             (in-memory)
3. Create metadata              (in-memory)
4. Write history/{version}.pvs          (atomic write-rename-fsync)
5. Write history/{version}.meta.json    (atomic write-rename-fsync)
6. Write lkg/config.pvs.tmp → rename    (atomic)
7. Write lkg/meta.json.tmp → rename     (atomic)
8. Update state.json                    (best-effort)
9. Wake long-poll waiters               (in-memory)
```

### Crash Points and Recovery

| Crash After Step | State | Recovery Action |
|------------------|-------|-----------------|
| 1-3 (validation) | No files written | Version NOT incremented ✅ |
| 4-5 (history) | Orphaned history entry | Safe to ignore, can cleanup manually |
| 6 (partial LKG) | Artifact without metadata | Delete artifact, recover from history |
| 7 (complete LKG) | Metadata without state.json | Derive version from LKG ✅ |
| 8 (state.json stale) | LKG complete, cache stale | Rewrite state.json from LKG ✅ |
| 9 (after success) | Normal case | All files consistent ✅ |

### Rollback Policy

- Rollback is **best-effort cleanup only** (not a safety requirement)
- If steps 4-7 fail: attempt to delete history entry (may fail, orphans are safe)
- Version NOT incremented on failure

---

## Automatic Recovery on Startup

### Repair Sequence

On every startup, the relay executes `repair_lkg()`:

```rust
1. Check if lkg/meta.json exists
   - YES: Load version → go to step 2
   - NO: Check if lkg/config.pvs exists
     - YES: Attempt recovery (go to step 2b)
     - NO: Set current_version = 0, skip to step 3

2. Verify lkg/config.pvs exists
   - YES: LKG complete, go to step 3
   - NO: Metadata without artifact → attempt recovery (go to step 2b)

2b. Attempt LKG recovery from history:
   - Load version from lkg/meta.json (if exists) or scan history for max version
   - Check if history/{version}.pvs and history/{version}.meta.json exist
   - If YES: Copy history files to lkg/ (restore)
   - If NO: FATAL ERROR ("LKG corruption and no history recovery source")

3. Verify state.json matches lkg/meta.json version
   - MISMATCH or MISSING: Rewrite state.json from lkg/meta.json

4. Scan history/ for orphaned/corrupt entries
   - Log warnings for orphans (version > current_version)
   - Log warnings for corrupt entries (missing .pvs or .meta.json)
   - Do NOT auto-delete (manual GC only)
```

### Log Messages During Recovery

**Normal startup (no corruption):**
```
INFO  Loaded LKG metadata: version=5
INFO  State.json verified: version=5
INFO  History scan: 5 versions, 0 orphans, 0 corrupt
```

**Startup with orphans:**
```
WARN  history entry version 6 exceeds LKG version 5
INFO  History scan: 6 versions, 1 orphans, 0 corrupt
```

**Startup with state.json mismatch:**
```
WARN  State.json version 3 does not match LKG version 5, rewriting
INFO  State.json updated: version=5
```

**Startup with LKG corruption:**
```
ERROR Failed to repair LKG: LKG corruption and no history recovery source
```

---

## Recovery Scenarios

### Scenario 1: Crash During Validation

**Crash Point:** After step 1-3 (before any file writes)

**State:**
- No files written to disk
- Version unchanged

**Recovery:**
- Automatic: None needed
- Version remains at previous value
- Next publish will retry with same version number

**Example:**
```bash
# Before crash
curl http://relay:8080/v1/status | jq .current_version
# Output: 5

# After crash and restart
curl http://relay:8080/v1/status | jq .current_version
# Output: 5 (unchanged)
```

---

### Scenario 2: Crash After History Write

**Crash Point:** After steps 4-5 (history written, LKG not promoted)

**State:**
```
history/
  0000000006.pvs        ← Written
  0000000006.meta.json  ← Written
lkg/
  config.pvs            ← Still version 5
  meta.json             ← Still version 5
```

**Recovery:**
- Automatic: Orphan detected on startup
- Current version remains at 5
- History entry for version 6 becomes orphaned

**Log output:**
```
WARN  history entry version 6 exceeds LKG version 5
```

**Manual cleanup (optional):**
```bash
cd /var/lib/pavis-relay/history
rm 0000000006.pvs 0000000006.meta.json
```

---

### Scenario 3: Crash After LKG Artifact Write

**Crash Point:** After step 6 (lkg/config.pvs written, lkg/meta.json missing)

**State:**
```
lkg/
  config.pvs      ← Written (version 6 bytes)
  meta.json       ← Missing or still version 5
history/
  0000000006.pvs        ← Written
  0000000006.meta.json  ← Written
```

**Recovery:**
- Automatic: `repair_lkg()` detects incomplete LKG
- Attempts recovery from history (step 2b)
- Copies `history/0000000006.meta.json` to `lkg/meta.json`
- LKG becomes consistent

**Log output:**
```
INFO  Attempting LKG recovery from history version 6
INFO  Restored lkg/meta.json from history/0000000006.meta.json
```

**Manual recovery (if automatic fails):**
```bash
cd /var/lib/pavis-relay

# Find max history version
MAX_VERSION=$(ls history/*.pvs | sed 's/.*\/\([0-9]*\)\.pvs/\1/' | sort -n | tail -1)

# Copy from history to LKG
cp "history/${MAX_VERSION}.meta.json" lkg/meta.json
cp "history/${MAX_VERSION}.pvs" lkg/config.pvs

# Restart relay
sudo systemctl restart pavis-relay
```

---

### Scenario 4: Crash After LKG Promotion

**Crash Point:** After step 7 (LKG complete, state.json stale)

**State:**
```
lkg/
  config.pvs      ← Version 6
  meta.json       ← Version 6
state.json        ← Still version 5 or missing
```

**Recovery:**
- Automatic: `state.json` rewritten from `lkg/meta.json`
- Current version becomes 6

**Log output:**
```
INFO  State.json version 5 does not match LKG version 6, rewriting
INFO  State.json updated: version=6
```

**No manual action needed.**

---

### Scenario 5: Corrupt State.json

**Symptoms:**
- `state.json` contains invalid JSON
- `state.json` has version > LKG version (impossible state)

**Example:**
```bash
# state.json contains corrupted data
cat /var/lib/pavis-relay/state.json
# Output: {"current_version": 999}  ← Higher than LKG

# LKG metadata
jq .version /var/lib/pavis-relay/lkg/meta.json
# Output: 5
```

**Recovery:**
- Automatic: Relay ignores state.json if it fails to parse or has impossible version
- Derives version from `lkg/meta.json`
- Rewrites `state.json`

**Manual recovery:**
```bash
# Delete corrupt state.json
rm /var/lib/pavis-relay/state.json

# Restart relay (will regenerate from LKG)
sudo systemctl restart pavis-relay

# Verify
jq . /var/lib/pavis-relay/state.json
```

---

### Scenario 6: Orphaned LKG Artifact

**State:**
```
lkg/
  config.pvs      ← Exists
  meta.json       ← Missing
history/
  (empty or partial)
```

**Recovery:**
- Automatic: `repair_lkg()` attempts recovery from history
- If history has matching version → restore metadata
- If no history → delete orphaned artifact, default to version 0

**Log output (successful recovery):**
```
INFO  Attempting LKG recovery from history version 3
INFO  Restored lkg/meta.json from history/0000000003.meta.json
```

**Log output (no history available):**
```
ERROR Failed to repair LKG: LKG corruption and no history recovery source
```

**Manual recovery:**
```bash
# If history exists
LAST_VERSION=$(ls /var/lib/pavis-relay/history/*.pvs | tail -1 | sed 's/.*\/\([0-9]*\)\.pvs/\1/')
cp "/var/lib/pavis-relay/history/${LAST_VERSION}.meta.json" /var/lib/pavis-relay/lkg/meta.json

# If no history, delete orphaned artifact
rm /var/lib/pavis-relay/lkg/config.pvs
```

---

## Critical Invariants

These invariants **MUST** hold at all times:

### Invariant 1: LKG Consistency

**Rule:** `lkg/meta.json` presence implies `lkg/config.pvs` exists

**Violation detection:**
```bash
if [ -f lkg/meta.json ] && [ ! -f lkg/config.pvs ]; then
  echo "VIOLATION: Metadata without artifact"
fi
```

**Recovery:** Copy from history or delete metadata

---

### Invariant 2: Metadata Authority

**Rule:** `lkg/meta.json` is the authoritative source for `current_version`

**Violation detection:**
```bash
LKG_VERSION=$(jq .version lkg/meta.json)
STATE_VERSION=$(jq .current_version state.json)

if [ "$STATE_VERSION" -gt "$LKG_VERSION" ]; then
  echo "VIOLATION: state.json version exceeds LKG version"
fi
```

**Recovery:** Rewrite `state.json` from `lkg/meta.json`

---

### Invariant 3: Orphan Safety

**Rule:** History entries with `version > current_version` are safe to ignore

**Detection:**
```bash
CURRENT=$(jq .version lkg/meta.json)
ls history/*.pvs | sed 's/.*\/\([0-9]*\)\.pvs/\1/' | while read v; do
  if [ "$v" -gt "$CURRENT" ]; then
    echo "Orphan: $v"
  fi
done
```

**Recovery:** Ignore or manually delete (no functional impact)

---

### Invariant 4: Checksum Integrity

**Rule:** LKG artifact checksum must match metadata checksum

**Verification:**
```bash
STORED=$(jq -r .checksum lkg/meta.json)
COMPUTED=$(sha256sum lkg/config.pvs | awk '{print "sha256:"$1}')

if [ "$STORED" != "$COMPUTED" ]; then
  echo "VIOLATION: Checksum mismatch!"
  echo "Stored:   $STORED"
  echo "Computed: $COMPUTED"
fi
```

**Recovery:** Delete LKG, recover from history, or re-publish

---

## Manual Recovery Procedures

### Full LKG Reconstruction from History

If LKG is completely corrupted:

```bash
#!/bin/bash
# reconstruct-lkg.sh

DATA_DIR="/var/lib/pavis-relay"
cd "$DATA_DIR"

# Find latest complete history entry
LATEST=""
for pvs in history/*.pvs; do
  version=$(basename "$pvs" .pvs)
  meta="history/${version}.meta.json"
  if [ -f "$meta" ]; then
    LATEST="$version"
  fi
done

if [ -z "$LATEST" ]; then
  echo "ERROR: No complete history entries found"
  exit 1
fi

echo "Reconstructing LKG from version $LATEST"

# Copy from history to LKG
cp "history/${LATEST}.pvs" lkg/config.pvs
cp "history/${LATEST}.meta.json" lkg/meta.json

# Delete state.json (will be regenerated)
rm -f state.json

# Verify checksums
STORED=$(jq -r .checksum lkg/meta.json)
COMPUTED=$(sha256sum lkg/config.pvs | awk '{print "sha256:"$1}')

if [ "$STORED" = "$COMPUTED" ]; then
  echo "✓ LKG reconstructed successfully"
  echo "  Version: $LATEST"
  echo "  Checksum: $STORED"
else
  echo "✗ Checksum mismatch after reconstruction!"
  exit 1
fi
```

---

### Verifying File Consistency

```bash
#!/bin/bash
# verify-consistency.sh

DATA_DIR="/var/lib/pavis-relay"
cd "$DATA_DIR"

echo "=== LKG Verification ==="

# Check LKG exists
if [ ! -f lkg/meta.json ]; then
  echo "✗ lkg/meta.json missing"
  exit 1
fi

if [ ! -f lkg/config.pvs ]; then
  echo "✗ lkg/config.pvs missing"
  exit 1
fi

# Verify checksum
STORED=$(jq -r .checksum lkg/meta.json)
COMPUTED=$(sha256sum lkg/config.pvs | awk '{print "sha256:"$1}')

if [ "$STORED" = "$COMPUTED" ]; then
  echo "✓ LKG checksum verified: $STORED"
else
  echo "✗ LKG checksum mismatch!"
  echo "  Stored:   $STORED"
  echo "  Computed: $COMPUTED"
  exit 1
fi

# Check state.json
LKG_VERSION=$(jq .version lkg/meta.json)
if [ -f state.json ]; then
  STATE_VERSION=$(jq .current_version state.json)
  if [ "$STATE_VERSION" = "$LKG_VERSION" ]; then
    echo "✓ state.json version matches LKG: $STATE_VERSION"
  else
    echo "⚠ state.json version $STATE_VERSION != LKG version $LKG_VERSION"
    echo "  (Will be rewritten on startup)"
  fi
else
  echo "⚠ state.json missing (Will be created on startup)"
fi

echo ""
echo "=== History Verification ==="

ORPHAN_COUNT=0
CORRUPT_COUNT=0

for pvs in history/*.pvs; do
  [ -f "$pvs" ] || continue
  version=$(basename "$pvs" .pvs)
  meta="history/${version}.meta.json"

  if [ ! -f "$meta" ]; then
    echo "✗ Corrupt: $version (missing metadata)"
    CORRUPT_COUNT=$((CORRUPT_COUNT + 1))
    continue
  fi

  if [ "$version" -gt "$LKG_VERSION" ]; then
    echo "⚠ Orphan: $version (exceeds LKG version $LKG_VERSION)"
    ORPHAN_COUNT=$((ORPHAN_COUNT + 1))
  fi
done

echo ""
echo "Summary:"
echo "  LKG Version:    $LKG_VERSION"
echo "  History Count:  $(ls history/*.pvs 2>/dev/null | wc -l)"
echo "  Orphans:        $ORPHAN_COUNT"
echo "  Corrupt:        $CORRUPT_COUNT"

if [ "$ORPHAN_COUNT" -eq 0 ] && [ "$CORRUPT_COUNT" -eq 0 ]; then
  echo "✓ All checks passed"
  exit 0
else
  echo "⚠ Issues detected (see above)"
  exit 1
fi
```

---

## Debugging Tools

### Inspecting State

```bash
# Current version (from state.json cache)
jq .current_version /var/lib/pavis-relay/state.json

# Current version (from authoritative LKG)
jq .version /var/lib/pavis-relay/lkg/meta.json

# LKG checksum
jq .checksum /var/lib/pavis-relay/lkg/meta.json

# LKG timestamp
jq .published_at /var/lib/pavis-relay/lkg/meta.json
```

### Comparing History Entries

```bash
# List all history versions
ls -1 /var/lib/pavis-relay/history/*.pvs | \
  sed 's/.*\/\([0-9]*\)\.pvs/\1/' | sort -n

# Compare checksums between versions
jq .checksum /var/lib/pavis-relay/history/0000000001.meta.json
jq .checksum /var/lib/pavis-relay/history/0000000002.meta.json

# If checksums match → identical configs (idempotent publish)
```

### Simulating Crashes

**For testing recovery logic:**

```bash
# Simulate crash after history write
sudo systemctl stop pavis-relay
rm /var/lib/pavis-relay/lkg/meta.json
sudo systemctl start pavis-relay
# Should trigger recovery from history

# Simulate state.json corruption
sudo systemctl stop pavis-relay
echo '{"current_version": 999}' > /var/lib/pavis-relay/state.json
sudo systemctl start pavis-relay
# Should rewrite from LKG
```

---

## Recovery Decision Tree

```
Start
  │
  ├─ Is lkg/meta.json present?
  │   ├─ YES → Is lkg/config.pvs present?
  │   │   ├─ YES → Load version from LKG ✓
  │   │   └─ NO → Attempt recovery from history → If success ✓, else FATAL ✗
  │   │
  │   └─ NO → Is lkg/config.pvs present?
  │       ├─ YES → Attempt recovery from history → If success ✓, else delete artifact + version=0
  │       └─ NO → Version = 0 (bootstrap) ✓
  │
  ├─ Is state.json present and valid?
  │   ├─ YES → Does it match LKG version?
  │   │   ├─ YES → Fast path ✓
  │   │   └─ NO → Rewrite from LKG ✓
  │   │
  │   └─ NO → Create from LKG ✓
  │
  └─ Scan history for orphans/corrupt
      ├─ Orphans found → Log warnings ⚠
      ├─ Corrupt found → Log warnings ⚠
      └─ Continue startup ✓
```

---

## FAQ

**Q: Can I delete orphaned history entries safely?**

A: Yes, orphaned entries (version > LKG version) are safe to delete. They represent incomplete publishes that never succeeded.

---

**Q: What if both LKG and history are corrupted?**

A: FATAL error. You must restore from backup or re-publish from source configuration.

---

**Q: Can I manually edit lkg/meta.json?**

A: **NO**. Never manually edit metadata files. Checksum verification will fail. Always use proper publish flow.

---

**Q: Why doesn't the relay auto-delete orphans?**

A: Conservative design. Orphans are harmless and may be useful for forensic analysis. Manual cleanup gives operators control.

---

**Q: Can state.json have a higher version than LKG?**

A: **NO**. This violates invariants. If detected, state.json is rewritten from LKG.

---

**Q: What happens if I delete state.json?**

A: Safe. Relay will regenerate it from lkg/meta.json on next startup.

---

## See Also

- [Operational Guide](relay.md)
- [API Reference](../api/relay.md)
- [Architecture Documentation](../../ARCHITECTURE.md)
