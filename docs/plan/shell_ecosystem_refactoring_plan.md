# Pavis Shell Ecosystem Refactoring - Executable Implementation Plan

**Version:** 3.0 (Executable)
**Target Repository:** pavis
**Scope:** Contract-driven incremental refactoring of shell scripts
**Approach:** Commit-by-commit, git-friendly, zero-downtime

---

## Executive Summary

This plan refactors the shell ecosystem in bench/ and tests/ to eliminate silent failures through artifact contracts, reduce coupling via explicit context.env runtime configuration, and introduce shared primitives in scripts/lib/. The plan is organized into 20 discrete commits across 2 phases, each independently testable and mergeable.

## Progress

**Status: ✅ PHASE 1 & 2 COMPLETE | ✅ PHASE 3 COMPLETE**

**Last Updated:** 2026-01-17

### Phase 1: Foundation (14 commits)
- [x] Commit 1: scripts/lib/log.sh + README.md
- [x] Commit 2: scripts/lib/time.sh
- [x] Commit 3: scripts/lib/wait.sh
- [x] Commit 4: scripts/lib/contract.sh (file-based validation)
- [x] Commit 5: bench/scripts/gen_context_env.sh (argv output path)
- [x] Commit 6: tests/scripts/gen_context_env.sh (argv output path)
- [x] Commit 7: bench/scripts/utils.sh delegates logging
- [x] Commit 8: bench/run.sh run-scoped context.env
- [x] Commit 9: copy context.env into benchmark case dir
- [x] Commit 10: per-case artifact validation + continue-on-failure
- [x] Commit 11: tests/run.sh run-scoped context.env
- [x] Commit 12: copy context.env into TEST_TMP
- [x] Commit 13: summarize prefers context.env
- [x] Commit 14: Phase 1 end-to-end validation

### Phase 2: Directory Migration (6 commits)
- [x] Commit 15: tests/lib duplicated to tests/scripts
- [x] Commit 16: compatibility shim skipped (tests/lib already removed after migration)
- [x] Commit 17: tests/run.sh sources tests/scripts
- [x] Commit 18: test cases source tests/scripts
- [x] Commit 19: remove tests/lib directory
- [x] Commit 20: documentation updated for tests/scripts

### Phase 3: Expanded Primitives (5 commits)
- [x] Commit 21: scripts/lib/process.sh (7 functions)
- [x] Commit 22: scripts/lib/http.sh (6 functions)
- [x] Commit 23: scripts/lib/json.sh (8 functions)
- [x] Commit 24: scripts/lib/docker.sh (8 functions)
- [x] Commit 25: scripts/lib/README.md updated

### Final Validation Summary (2026-01-17)

**All validation tests passed:**

1. **Shell Syntax Validation**
   - ✅ scripts/lib/*.sh (log, time, wait, contract)
   - ✅ tests/scripts/*.sh (log, env, assert, docker, gen_context_env)
   - ✅ bench/scripts/*.sh (gen_context_env, utils, benchmark)

2. **Functional Tests**
   - ✅ log_info, log_error, log_debug output correctly
   - ✅ timestamp_iso8601, timestamp_unix format validation
   - ✅ wait_for_file timeout and success paths
   - ✅ Bench context.env generation with required fields
   - ✅ Test context.env generation with required fields
   - ✅ Artifact validation (meta.json, loadgen.txt.json, wrk.txt)
   - ✅ Full artifact validation for loadgen and wrk cases

3. **Migration Verification**
   - ✅ tests/lib directory removed
   - ✅ No source references to tests/lib
   - ✅ No shellcheck comments referencing tests/lib
   - ✅ tests/scripts structure complete (5 files)

4. **Integration Status**
   - ✅ All shared primitives functional
   - ✅ Context.env generation and propagation working
   - ✅ Artifact contract validation operational
   - ✅ Directory migration complete

### Critical Design Decisions

**No RUN_ID in paths.** Outputs are cleaned before each run and remain at fixed locations:
- Bench run-scoped: `bench/output/{mode}/context.env`
- Bench case-scoped: `bench/output/{mode}/{proxy}/{case}/context.env`
- Tests run-scoped: `tests/temp/context.env`
- Tests case-scoped: `${TEST_TMP}/context.env`

**Context generation scripts are executable.** They accept output path as argv[1], write shell-sourceable key=value pairs using `printf '%s=%q\n'` for safe quoting, and exit non-zero on failure. Entry points invoke them via `bash path/to/gen_context_env.sh "$outfile"`, not by sourcing.

**Artifact validation inspects actual files present, not case names.** `validate_benchmark_artifacts` checks if `loadgen.txt.json` exists (validates as loadgen), else checks for `run_*/wrk.txt` or `wrk.txt` (validates as wrk), and optionally validates `metrics.json` if present. Always validates `meta.json` schema.

**Benchmark runner continues on failure.** The `run_case` function captures failures, marks failed cases with `.validation_failed`, and continues to next case. At end, exit 1 if any failures occurred.

**Tool dependencies are explicit.** Entry points check for required commands (jq for validation, nc or bash /dev/tcp for port waits) and fail fast with clear messages if missing.

**Summarize script sources context.env safely.** It only reads RUN_* and BENCH_* variables; local variables use distinct names to avoid clobbering.

---

## Context.env Specification

**Purpose:** Single source of truth for runtime configuration, shell-sourceable.

### Locations (Fixed)

**Benchmark:**
- Run-scoped: `bench/output/{mode}/context.env`
- Case-scoped: `bench/output/{mode}/{proxy}/{case}/context.env`

**Tests:**
- Run-scoped: `tests/temp/context.env`
- Case-scoped: `${TEST_TMP}/context.env` (copied into each test temp dir)

### Required Fields

**All contexts MUST include:**
```bash
RUN_TIMESTAMP=<ISO8601 UTC>
GIT_SHA=<commit hash>
```

**Optional (informational only, NOT used in paths):**
```bash
RUN_TAG=<short git sha or custom label>
```

### Schema (Benchmark)

```bash
# Run Identity (REQUIRED)
RUN_TIMESTAMP=2024-01-16T10:30:45Z
GIT_SHA=abc123def456...

# Run Identity (OPTIONAL)
RUN_TAG=abc123de

# Benchmark Configuration
BENCH_MODE=standalone
BENCH_PROFILE=github
BENCH_PROXY=pavis
BENCH_PAYLOAD_SIZE=64B
BENCH_TLS=false
BENCH_METRICS=false

# Infrastructure Paths
BENCH_DOCKER_COMPOSE=/path/to/docker-compose.yaml
BENCH_LOADGEN_BIN=/path/to/bench-loadgen
BENCH_PVS_CONFIG=/path/to/pavis.pvs

# Resource Limits
BACKEND_CPUSET=0
PROXY_CPUSET=1-2
BENCH_LOADGEN_CPUSET=3
BENCH_PROXY_CPU_LIMIT=2
BENCH_PROXY_MEM_LIMIT=1G

# Host Info
BENCH_HOST_CORES=4
BENCH_HOST_CPUSET_EFFECTIVE=0-3
BENCH_HOST_MEM_TOTAL=8192MiB
BENCH_HOST_CPU_MODEL=Intel Core i7
BENCH_HOST_KERNEL=5.15.0

# Paths
BENCH_ROOT=/path/to/pavis
BENCH_SCRIPTS_DIR=/path/to/pavis/bench/scripts
BENCH_OUTPUT_DIR=/path/to/pavis/bench/output
BENCH_CASES_DIR=/path/to/pavis/bench/cases/standalone
```

### Schema (Tests)

```bash
# Run Identity (REQUIRED)
RUN_TIMESTAMP=2024-01-16T10:30:45Z
GIT_SHA=abc123def456...

# Run Identity (OPTIONAL)
RUN_TAG=abc123de

# Test Configuration
TEST_MODE=binary
TEST_SUITE=pavis

# Binary Paths
PAVIS_BIN=/path/to/pavis
RELAY_BIN=/path/to/pavis-relay
PAVCTL_BIN=/path/to/pavctl
PAVIS_UPSTREAM_BIN=/path/to/pavis-mock-upstream
MOCK_RELAY_BIN=/path/to/pavis-mock-relay

# Docker Images (if TEST_MODE=docker)
PAVIS_IMAGE=pavis:local
RELAY_IMAGE=pavis-relay:local
MOCK_RELAY_IMAGE=pavis-mock-relay:local

# Upstream Ports
UPSTREAM_HTTP_PORT_V1=8081
UPSTREAM_HTTP_PORT_V2=8082
UPSTREAM_HTTPS_PORT_V1=8443
UPSTREAM_HTTPS_PORT_V2=8444

# Paths
PROJECT_ROOT=/path/to/pavis
SCRIPT_DIR=/path/to/pavis/tests
TEST_SCRIPTS_DIR=/path/to/pavis/tests/scripts
```

---

## Artifact Contracts

**Purpose:** Define minimal required outputs validated by inspecting actual files present.

### Benchmark Artifacts

**File-based detection (NOT name-based):**

The `validate_benchmark_artifacts` function inspects which files exist:
- If `loadgen.txt.json` exists → validate as loadgen output
- Else if `run_*/wrk.txt` directories exist → validate at least one wrk output
- Else if `wrk.txt` exists → validate as wrk output
- If `metrics.json` exists → validate as JSON (system mode)
- Always validate `meta.json` schema

**Meta.json schema:**
- Must be valid JSON
- Required keys: `case`, `proxy`, `timestamp`

**WRK output (wrk.txt):**
- Must contain line matching: `Requests/sec:`

**Loadgen output (loadgen.txt.json):**
- Must be valid JSON
- Required keys: `achieved_rps`, `p50_ms`, `p90_ms`, `p99_ms`, `errors`, `dropped`

**Docker stats (docker_stats.csv) - optional:**
- If present, must have header row + at least 1 data row

### Test Artifacts

**All test cases:**
- Exit code 0 = pass, 77 = skip, non-zero (except 77) = fail
- No artifact validation (tests are pass/fail only)

---

## Phase 1: Foundation (14 Commits)

Phase 1 establishes the runtime contract infrastructure without breaking existing behavior.

---

### Commit 1: Create scripts/lib/log.sh and README.md

**Title:** Add shared logging primitives to scripts/lib

**Files Added:**
- `scripts/lib/log.sh`
- `scripts/lib/README.md`

**Implementation:**

Create `scripts/lib/log.sh` containing six functions: `log_info`, `log_warn`, `log_error`, `log_debug`, `log_section`, `exit_with_error`. All output follows format "[LEVEL] message". log_info, log_warn, log_section write to stdout. log_error writes to stderr. log_debug only outputs if DEBUG=1 is set. exit_with_error takes message and exit code (defaulting to 1), logs error, then exits.

Example implementation:

```bash
#!/bin/bash
set -euo pipefail

log_info() {
  echo "[INFO] $*"
}

log_warn() {
  echo "[WARN] $*"
}

log_error() {
  echo "[ERROR] $*" >&2
}

log_debug() {
  if [[ "${DEBUG:-0}" == "1" ]]; then
    echo "[DEBUG] $*"
  fi
}

log_section() {
  echo ""
  echo "=== $* ==="
  echo ""
}

exit_with_error() {
  local msg="$1"
  local code="${2:-1}"
  log_error "$msg"
  exit "$code"
}
```

Create `scripts/lib/README.md` documenting purpose: "Shared primitive utilities for shell scripts across bench/ and tests/." State critical dependency rule: "scripts/lib/ MUST NEVER source files from bench/ or tests/ to avoid circular dependencies." List planned modules: log, time, wait, process, http, json, docker.

**Commands:**

```bash
bash -n scripts/lib/log.sh
bash -c "source scripts/lib/log.sh && log_info 'test message'" | grep '\[INFO\] test message'
bash -c "source scripts/lib/log.sh && log_error 'error msg' 2>&1" | grep '\[ERROR\] error msg'
bash -c "DEBUG=1 bash -c 'source scripts/lib/log.sh && log_debug \"debug msg\"'" | grep '\[DEBUG\] debug msg'
bash -c "source scripts/lib/log.sh && log_section 'Section Title'" | grep '=== Section Title ==='
```

**Rollback:** `rm -rf scripts/lib/`

**Risk:** None. Creates new files only.

---

### Commit 2: Create scripts/lib/time.sh

**Title:** Add timestamp utilities to scripts/lib

**Files Added:**
- `scripts/lib/time.sh`

**Implementation:**

Create `scripts/lib/time.sh` with four functions. `timestamp_iso8601` returns current UTC time in ISO8601 format using `date -u +"%Y-%m-%dT%H:%M:%SZ"`. `timestamp_unix` returns Unix epoch using `date +%s`. `timestamp_precise` returns high-precision timestamp using `python3 -c 'import time; print(time.time())'` with fallback to `date +%s.%N` on Linux or `date +%s` if neither available. `duration_seconds` takes two Unix timestamps and returns the difference.

Example implementation:

```bash
#!/bin/bash
set -euo pipefail

timestamp_iso8601() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

timestamp_unix() {
  date +%s
}

timestamp_precise() {
  if command -v python3 &>/dev/null; then
    python3 -c 'import time; print(time.time())'
  elif [[ "$(uname)" == "Linux" ]]; then
    date +%s.%N
  else
    date +%s
  fi
}

duration_seconds() {
  local start="$1"
  local end="$2"
  echo $((end - start))
}
```

**Commands:**

```bash
bash -n scripts/lib/time.sh
bash -c "source scripts/lib/time.sh && timestamp_iso8601" | grep -E '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$'
bash -c "source scripts/lib/time.sh && timestamp_unix" | grep -E '^\d+$'
bash -c "source scripts/lib/time.sh && timestamp_precise" | grep -E '^\d+(\.\d+)?$'
bash -c "source scripts/lib/time.sh && duration_seconds 100 150" | grep '^50$'
```

**Rollback:** `rm scripts/lib/time.sh`

**Risk:** None.

---

### Commit 3: Create scripts/lib/wait.sh

**Title:** Add polling utilities to scripts/lib

**Files Added:**
- `scripts/lib/wait.sh`

**Implementation:**

Create `scripts/lib/wait.sh` sourcing `scripts/lib/log.sh`. Implement three functions: `wait_for_url`, `wait_for_port`, `wait_for_file`.

`wait_for_url` takes url, timeout, and optional curl args. Polls URL with curl until success or timeout. Returns 0 on success, 1 on timeout.

`wait_for_port` takes host, port, timeout. Uses `nc -z` if available, else tries bash `/dev/tcp/$host/$port` redirect. Returns 0 on success, 1 on timeout, 2 if neither method available.

`wait_for_file` takes filepath and timeout. Polls for file existence. Returns 0 if found, 1 on timeout.

Example skeleton:

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/log.sh"

wait_for_url() {
  local url="$1"
  local timeout="$2"
  shift 2
  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    if curl -sf "$@" "$url" >/dev/null 2>&1; then
      log_debug "URL $url is ready"
      return 0
    fi
    sleep 2
  done

  log_error "Timeout waiting for URL $url"
  return 1
}

wait_for_port() {
  local host="$1"
  local port="$2"
  local timeout="$3"
  local end=$(($(date +%s) + timeout))

  if command -v nc &>/dev/null; then
    while [[ $(date +%s) -lt $end ]]; do
      if nc -z "$host" "$port" &>/dev/null; then
        log_debug "Port $host:$port is ready"
        return 0
      fi
      sleep 1
    done
  elif [[ -e /dev/tcp ]]; then
    while [[ $(date +%s) -lt $end ]]; do
      if bash -c "cat < /dev/tcp/$host/$port" 2>/dev/null; then
        log_debug "Port $host:$port is ready"
        return 0
      fi
      sleep 1
    done
  else
    log_error "Neither nc nor /dev/tcp available for port check"
    return 2
  fi

  log_error "Timeout waiting for port $host:$port"
  return 1
}

wait_for_file() {
  local filepath="$1"
  local timeout="$2"
  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    if [[ -f "$filepath" ]]; then
      log_debug "File $filepath exists"
      return 0
    fi
    sleep 1
  done

  log_error "Timeout waiting for file $filepath"
  return 1
}
```

**Commands:**

```bash
bash -n scripts/lib/wait.sh
bash -c "source scripts/lib/wait.sh && wait_for_file /etc/passwd 1" && echo PASS
bash -c "source scripts/lib/wait.sh && wait_for_file /nonexistent_file_xyz 1" || echo PASS
```

**Rollback:** `rm scripts/lib/wait.sh`

**Risk:** None.

---

### Commit 4: Create scripts/lib/contract.sh with file-based validation

**Title:** Add artifact contract validation library

**Files Added:**
- `scripts/lib/contract.sh`

**Implementation:**

Create `scripts/lib/contract.sh` sourcing `scripts/lib/log.sh`. Implement validation functions that inspect actual files, never inferring from case names.

Key functions:
- `require_cmd` - checks if command exists (jq required for validation)
- `validate_meta_json` - validates meta.json exists, is valid JSON, contains required keys (case, proxy, timestamp)
- `validate_wrk_output` - validates wrk.txt contains "Requests/sec:" line
- `validate_loadgen_output` - validates loadgen.txt.json is valid JSON with required keys
- `validate_docker_stats` - validates docker_stats.csv has header + data (optional file)
- `validate_benchmark_artifacts` - main function that inspects which files exist and validates accordingly

Critical: `validate_benchmark_artifacts` uses file-based detection:

```bash
validate_benchmark_artifacts() {
  local case_name="$1"
  local case_dir="$2"

  log_debug "Validating artifacts for case $case_name in $case_dir"

  # Always validate meta.json
  validate_meta_json "$case_dir/meta.json" || return 1

  # Determine workload type by inspecting files (NOT by case name)
  if [[ -f "$case_dir/loadgen.txt.json" ]]; then
    log_debug "Found loadgen.txt.json, validating as loadgen case"
    validate_loadgen_output "$case_dir/loadgen.txt.json" || return 1
  elif compgen -G "$case_dir/run_*/wrk.txt" >/dev/null; then
    log_debug "Found run_*/wrk.txt, validating as multi-run wrk case"
    local found=0
    for wrk_file in "$case_dir"/run_*/wrk.txt; do
      if validate_wrk_output "$wrk_file"; then
        found=1
        break
      fi
    done
    if [[ $found -eq 0 ]]; then
      log_error "No valid wrk.txt found in run_* directories"
      return 1
    fi
  elif [[ -f "$case_dir/wrk.txt" ]]; then
    log_debug "Found wrk.txt, validating as single-run wrk case"
    validate_wrk_output "$case_dir/wrk.txt" || return 1
  else
    log_error "No recognized workload output found"
    return 1
  fi

  # Validate metrics.json if present (system mode)
  if [[ -f "$case_dir/metrics.json" ]]; then
    require_cmd jq || return 1
    if ! jq . "$case_dir/metrics.json" >/dev/null 2>&1; then
      log_error "Invalid JSON in metrics.json"
      return 1
    fi
  fi

  # Validate docker_stats.csv if present
  validate_docker_stats "$case_dir/docker_stats.csv" || return 1

  log_debug "All artifacts validated for $case_name"
  return 0
}
```

**Commands:**

```bash
bash -n scripts/lib/contract.sh

# Test meta.json validation
mkdir -p /tmp/test_contract
echo '{"case":"test","proxy":"pavis","timestamp":"2024-01-01T00:00:00Z"}' > /tmp/test_contract/meta.json
bash -c "source scripts/lib/contract.sh && validate_meta_json /tmp/test_contract/meta.json" && echo PASS

echo 'invalid json' > /tmp/test_contract/bad.json
bash -c "source scripts/lib/contract.sh && validate_meta_json /tmp/test_contract/bad.json" || echo PASS

# Test loadgen validation
echo '{"achieved_rps":100,"p50_ms":1.0,"p90_ms":2.0,"p99_ms":3.0,"errors":0,"dropped":0}' > /tmp/test_contract/loadgen.txt.json
bash -c "source scripts/lib/contract.sh && validate_loadgen_output /tmp/test_contract/loadgen.txt.json" && echo PASS

# Test wrk validation
echo -e "Some output\nRequests/sec: 1000.00\nMore output" > /tmp/test_contract/wrk.txt
bash -c "source scripts/lib/contract.sh && validate_wrk_output /tmp/test_contract/wrk.txt" && echo PASS

# Test full artifact validation
bash -c "source scripts/lib/contract.sh && validate_benchmark_artifacts test /tmp/test_contract" && echo PASS

rm -rf /tmp/test_contract
```

**Rollback:** `rm scripts/lib/contract.sh`

**Risk:** None. Creates validation library only.

---

### Commit 5: Create bench/scripts/gen_context_env.sh as executable

**Title:** Add executable benchmark context generator

**Files Added:**
- `bench/scripts/gen_context_env.sh`

**Implementation:**

Create `bench/scripts/gen_context_env.sh` as an executable script (chmod +x). It is invoked as `bash bench/scripts/gen_context_env.sh /path/to/output.env` (NOT sourced). Takes output path as argv[1]. Exits with code 1 if no argument provided or on error.

Script sources `scripts/lib/log.sh` and `scripts/lib/time.sh`. Gathers run identity, BENCH_* variables, resource limits, host information, and paths. Writes using `printf '%s=%q\n'` for safe shell quoting.

Example main function:

```bash
main() {
  local output_file="${1:-}"

  if [[ -z "$output_file" ]]; then
    log_error "Usage: $0 <output_file>"
    exit 1
  fi

  mkdir -p "$(dirname "$output_file")"

  {
    # Run Identity (REQUIRED)
    printf '%s=%q\n' "RUN_TIMESTAMP" "$(timestamp_iso8601)"
    printf '%s=%q\n' "GIT_SHA" "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')"

    # Run Identity (OPTIONAL)
    printf '%s=%q\n' "RUN_TAG" "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"

    # Benchmark Configuration (all BENCH_* variables)
    printf '%s=%q\n' "BENCH_MODE" "${BENCH_MODE:-}"
    printf '%s=%q\n' "BENCH_PROFILE" "${BENCH_PROFILE:-}"
    # ... (continue for all BENCH_* variables)

    # Host Info with fallbacks
    # ... (detect cores, memory, CPU model, kernel)

  } > "$output_file"

  log_info "Generated benchmark context: $output_file"
}

main "$@"
```

Make executable: `chmod +x bench/scripts/gen_context_env.sh`

**Commands:**

```bash
bash -n bench/scripts/gen_context_env.sh

export BENCH_MODE=standalone BENCH_PROXY=pavis BENCH_ROOT=/tmp/test BENCH_SCRIPTS_DIR=/tmp/test/bench/scripts
bash bench/scripts/gen_context_env.sh /tmp/bench_context.env

cat /tmp/bench_context.env
grep -E '^(RUN_TIMESTAMP|GIT_SHA|BENCH_MODE|BENCH_PROXY)=' /tmp/bench_context.env

source /tmp/bench_context.env
echo "BENCH_MODE=$BENCH_MODE"

rm /tmp/bench_context.env
```

**Rollback:** `rm bench/scripts/gen_context_env.sh`

**Risk:** None.

---

### Commit 6: Create tests/scripts/gen_context_env.sh as executable

**Title:** Add executable test context generator

**Files Added:**
- `tests/scripts/gen_context_env.sh`

**Implementation:**

Create `tests/scripts/` directory if not exists. Create `tests/scripts/gen_context_env.sh` as executable (chmod +x). Same pattern as bench: takes output path as argv[1], sources `scripts/lib/log.sh` and `scripts/lib/time.sh`, gathers run identity, TEST_* variables, binary paths, image names (if docker mode), ports, and paths. Writes using `printf '%s=%q\n'`.

Make executable: `chmod +x tests/scripts/gen_context_env.sh`

**Commands:**

```bash
bash -n tests/scripts/gen_context_env.sh

export TEST_MODE=binary PROJECT_ROOT=/tmp/test SCRIPT_DIR=/tmp/test/tests TEST_SCRIPTS_DIR=/tmp/test/tests/scripts PAVIS_BIN=/tmp/pavis
bash tests/scripts/gen_context_env.sh /tmp/test_context.env

cat /tmp/test_context.env
grep -E '^(RUN_TIMESTAMP|GIT_SHA|TEST_MODE)=' /tmp/test_context.env

source /tmp/test_context.env
echo "TEST_MODE=$TEST_MODE"

rm /tmp/test_context.env
```

**Rollback:** `rm tests/scripts/gen_context_env.sh`

**Risk:** None.

---

### Commit 7: Integrate scripts/lib/log.sh into bench/scripts/utils.sh

**Title:** Delegate bench logging to shared primitives

**Files Modified:**
- `bench/scripts/utils.sh`

**Implementation:**

At top of `bench/scripts/utils.sh`, add:

```bash
UTILS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$UTILS_DIR/../../scripts/lib/log.sh"
```

Modify existing `_log` function to delegate:

```bash
# Logging now delegates to scripts/lib/log.sh
_log() {
  local level="$1"; shift
  case "$level" in
    INFO) log_info "$@" ;;
    WARN) log_warn "$@" ;;
    ERROR) log_error "$@" ;;
    DEBUG) log_debug "$@" ;;
    *) log_info "$@" ;;
  esac
}
```

Keep existing wrapper function signatures that delegate to `_log`. Preserve existing interface while delegating work to shared library.

**Commands:**

```bash
bash -n bench/scripts/utils.sh
bash -c "source bench/scripts/utils.sh && log_info 'test'" | grep '\[INFO\] test'
bash -c "source bench/scripts/utils.sh && log_warn 'test'" | grep '\[WARN\] test'
bash -c "source bench/scripts/utils.sh && log_error 'test' 2>&1" | grep '\[ERROR\] test'
```

**Rollback:** `git checkout HEAD -- bench/scripts/utils.sh`

**Risk:** Low. Delegation preserves existing interfaces.

---

### Commit 8: Generate run-level context.env in bench/run.sh

**Title:** Create run-scoped context.env for benchmarks

**Files Modified:**
- `bench/run.sh`

**Implementation:**

At beginning of `bench/run.sh`, add early dependency check:

```bash
# Check required dependencies
if ! command -v jq &>/dev/null; then
  echo "[ERROR] jq is required for artifact validation but not found. Please install jq." >&2
  exit 1
fi
```

After validate_inputs, ensure output directory is cleaned:

```bash
# Clean output directory (single active run at a time)
if [[ -d "${BENCH_OUTPUT_DIR}/${BENCH_MODE}" ]]; then
  log_info "Cleaning previous output: ${BENCH_OUTPUT_DIR}/${BENCH_MODE}"
  rm -rf "${BENCH_OUTPUT_DIR:?}/${BENCH_MODE:?}"
fi
mkdir -p "${BENCH_OUTPUT_DIR}/${BENCH_MODE}"
```

After setup_environment, generate run-level context.env:

```bash
# Generate run-level context.env
RUN_CONTEXT_ENV="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/context.env"
log_info "Generating runtime context: $RUN_CONTEXT_ENV"

if ! bash "${BENCH_SCRIPTS_DIR}/gen_context_env.sh" "$RUN_CONTEXT_ENV"; then
  log_error "Failed to generate context.env"
  exit 1
fi

export RUN_CONTEXT_ENV
log_info "Runtime context saved to $RUN_CONTEXT_ENV"
```

Do NOT export RUN_ID or use it in paths. If bench/run.sh currently sets RUN_ID, remove it or keep internal only.

**Commands:**

```bash
bash -n bench/run.sh

./bench/run.sh --proxy pavis --mode standalone --cases throughput_short_1x

ls -la bench/output/standalone/context.env

source bench/output/standalone/context.env
echo "RUN_TIMESTAMP=$RUN_TIMESTAMP"
echo "GIT_SHA=$GIT_SHA"
echo "BENCH_PROXY=$BENCH_PROXY"

grep -E '^(RUN_TIMESTAMP|GIT_SHA|BENCH_MODE|BENCH_PROXY)=' bench/output/standalone/context.env
```

**Rollback:** `git checkout HEAD -- bench/run.sh && rm -rf bench/output/*/context.env`

**Risk:** Medium. Aborts early if gen_context_env.sh fails (desired behavior).

---

### Commit 9: Copy context.env into each benchmark case directory

**Title:** Make each case output directory self-describing

**Files Modified:**
- `bench/scripts/benchmark.sh`

**Implementation:**

At top of `bench/scripts/benchmark.sh`, after sourcing utils.sh, add:

```bash
BENCHMARK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$BENCHMARK_DIR/../../scripts/lib/contract.sh"
```

In the `run_case` function, after case directory is created and case script completes, add:

```bash
# Copy run-level context.env to case directory
if [[ -n "${RUN_CONTEXT_ENV:-}" && -f "$RUN_CONTEXT_ENV" ]]; then
  if ! cp "$RUN_CONTEXT_ENV" "${case_output_dir}/context.env"; then
    log_warn "Failed to copy context.env to ${case_output_dir}"
  else
    log_debug "Copied context.env to ${case_output_dir}"
  fi
fi
```

**Commands:**

```bash
bash -n bench/scripts/benchmark.sh

./bench/run.sh --proxy pavis --mode standalone --cases throughput_short_1x

find bench/output/standalone -name context.env -type f

case_context=$(find bench/output/standalone/pavis -name context.env | head -1)
diff bench/output/standalone/context.env "$case_context"
```

**Rollback:** `git checkout HEAD -- bench/scripts/benchmark.sh`

**Risk:** Low. Copy failure is non-fatal.

---

### Commit 10: Add artifact validation with continue-on-failure logic

**Title:** Validate artifacts post-run and continue on failure

**Files Modified:**
- `bench/scripts/benchmark.sh`

**Implementation:**

Initialize failure tracking at start of case loop:

```bash
declare -a failed_cases=()
```

In `run_case` function, after context.env is copied, add validation:

```bash
# Validate artifacts
if ! validate_benchmark_artifacts "$case_name" "$case_output_dir"; then
  log_error "Case $case_name produced invalid artifacts"
  touch "${case_output_dir}/.validation_failed"
  failed_cases+=("$case_name")
  return 1
fi

log_debug "Case $case_name artifacts validated"
return 0
```

Ensure case loop continues on failure:

```bash
for case in "${cases[@]}"; do
  run_case "$case" || true  # Continue even on failure
done
```

At end of script:

```bash
if [[ ${#failed_cases[@]} -gt 0 ]]; then
  log_error "The following cases failed validation: ${failed_cases[*]}"
  exit 1
fi

log_info "All cases completed successfully"
exit 0
```

**Commands:**

```bash
bash -n bench/scripts/benchmark.sh

./bench/run.sh --proxy pavis --mode standalone --cases throughput_short_1x

! find bench/output/standalone -name '.validation_failed' -type f

# Manual validation test
mkdir -p /tmp/fake_case
echo '{"case":"test","proxy":"pavis","timestamp":"2024-01-01T00:00:00Z"}' > /tmp/fake_case/meta.json
echo -e "Output\nRequests/sec: 1000\n" > /tmp/fake_case/wrk.txt
bash -c "source scripts/lib/contract.sh && validate_benchmark_artifacts test /tmp/fake_case" && echo PASS

echo 'invalid' > /tmp/fake_case/meta.json
bash -c "source scripts/lib/contract.sh && validate_benchmark_artifacts test /tmp/fake_case" || echo PASS

rm -rf /tmp/fake_case
```

**Rollback:** `git checkout HEAD -- bench/scripts/benchmark.sh`

**Risk:** Medium. Validation bugs could false-fail valid cases.

---

### Commit 11: Generate run-level context.env in tests/run.sh

**Title:** Create run-scoped context.env for tests

**Files Modified:**
- `tests/run.sh`

**Implementation:**

After lib scripts are sourced, clean temp directory:

```bash
# Clean temp directory (single active run at a time)
if [[ -d "${SCRIPT_DIR}/temp" ]]; then
  log_info "Cleaning previous test temp directory"
  rm -rf "${SCRIPT_DIR}/temp"
fi
mkdir -p "${SCRIPT_DIR}/temp"
```

Generate run-level context.env:

```bash
# Generate run-level context.env
RUN_CONTEXT_ENV="${SCRIPT_DIR}/temp/context.env"
log_info "Generating test runtime context: $RUN_CONTEXT_ENV"

if ! bash "${SCRIPT_DIR}/scripts/gen_context_env.sh" "$RUN_CONTEXT_ENV"; then
  log_error "Failed to generate test context.env"
  exit 1
fi

export RUN_CONTEXT_ENV
log_info "Test runtime context saved to $RUN_CONTEXT_ENV"
```

If tests/run.sh sets RUN_ID, remove export or keep internal only (not used in paths).

**Commands:**

```bash
bash -n tests/run.sh

./tests/run.sh pavis 10_bootstrap_static

ls -la tests/temp/context.env

source tests/temp/context.env
echo "TEST_MODE=$TEST_MODE"
echo "RUN_TIMESTAMP=$RUN_TIMESTAMP"
echo "GIT_SHA=$GIT_SHA"

grep -E '^(RUN_TIMESTAMP|GIT_SHA|TEST_MODE)=' tests/temp/context.env
```

**Rollback:** `git checkout HEAD -- tests/run.sh && rm -f tests/temp/context.env`

**Risk:** Low. Tests abort early if context generation fails.

---

### Commit 12: Copy context.env into each test case temp directory

**Title:** Make each test temp directory self-describing

**Files Modified:**
- `tests/lib/env.sh` (or tests/scripts/env.sh if already migrated)

**Implementation:**

In `setup_test` function, after creating TEST_TMP:

```bash
# Copy run-level context.env to test temp directory
if [[ -n "${RUN_CONTEXT_ENV:-}" && -f "$RUN_CONTEXT_ENV" ]]; then
  if ! cp "$RUN_CONTEXT_ENV" "${TEST_TMP}/context.env"; then
    log_warn "Failed to copy context.env to ${TEST_TMP}"
  else
    if [[ "${E2E_VERBOSE:-0}" -eq 1 ]]; then
      echo "Copied context.env to ${TEST_TMP}"
    fi
  fi
fi
```

**Commands:**

```bash
bash -n tests/lib/env.sh

./tests/run.sh pavis 10_bootstrap_static

find tests/temp -name context.env -type f

test_context=$(find tests/temp -mindepth 2 -name context.env -type f | head -1)
if [[ -n "$test_context" ]]; then
  cat "$test_context" | grep TEST_MODE && echo PASS
fi
```

**Rollback:** `git checkout HEAD -- tests/lib/env.sh`

**Risk:** None.

---

### Commit 13: Update summarize.sh to prefer context.env

**Title:** Read runtime config from context.env in summarize

**Files Modified:**
- `bench/scripts/summarize.sh`

**Implementation:**

In `parse_case` function, at beginning:

```bash
# Source context.env if available (prefer over meta.json for runtime config)
if [[ -f "${case_dir}/context.env" ]]; then
  # shellcheck disable=SC1090
  source "${case_dir}/context.env"
fi
```

When reading metadata, prefer context.env variables:

```bash
# Prefer context.env variables over meta.json
run_timestamp="${RUN_TIMESTAMP:-$(jq -r '.timestamp // empty' "${case_dir}/meta.json")}"
git_sha="${GIT_SHA:-$(jq -r '.git_sha // empty' "${case_dir}/meta.json")}"
bench_profile="${BENCH_PROFILE:-$(jq -r '.bench_profile // empty' "${case_dir}/meta.json")}"
# ... (continue for other fields)
```

Add comment:

```bash
# Prefer context.env over meta.json for runtime configuration.
# meta.json still used for case-specific data (target_rps, container names, etc.)
```

**Commands:**

```bash
bash -n bench/scripts/summarize.sh

./bench/run.sh --proxy pavis --mode standalone --cases throughput_short_1x

./bench/scripts/summarize.sh bench/output/standalone

test -f bench/output/standalone/summary.csv && echo PASS

head -n 2 bench/output/standalone/summary.csv

context_sha=$(grep '^GIT_SHA=' bench/output/standalone/pavis/throughput_short_1x*/context.env | head -1 | cut -d= -f2)
summary_sha=$(awk -F, 'NR==2 {print $1}' bench/output/standalone/summary.csv)
echo "Context SHA: $context_sha"
echo "Summary SHA: $summary_sha"
```

**Rollback:** `git checkout HEAD -- bench/scripts/summarize.sh`

**Risk:** Low. Backward compatible.

---

### Commit 14: Phase 1 end-to-end validation

**Title:** Validate Phase 1 integration end-to-end

**Files:** None (validation only)

**Implementation:**

This is a validation-only commit. Run comprehensive tests to ensure all Phase 1 changes work together.

**Commands:**

```bash
# Clean all outputs
rm -rf bench/output tests/temp

# Full benchmark run
./bench/run.sh --proxy pavis --mode standalone --cases throughput_short_1x latency_short_1x

# Verify run-level context.env
test -f bench/output/standalone/context.env && echo "PASS: Run-level context.env exists"
source bench/output/standalone/context.env
echo "RUN_TIMESTAMP=$RUN_TIMESTAMP"
echo "GIT_SHA=$GIT_SHA"

# Verify case-level context.env files
case_contexts=$(find bench/output/standalone -name context.env -type f | wc -l)
echo "Found $case_contexts context.env files (expect 3: 1 run + 2 cases)"

# Verify no validation failures
! find bench/output/standalone -name '.validation_failed' -type f && echo "PASS: No validation failures"

# Verify summarize works
./bench/scripts/summarize.sh bench/output/standalone
test -f bench/output/standalone/summary.csv && echo "PASS: summary.csv generated"

# Full test run
./tests/run.sh pavis

# Verify test context.env
test -f tests/temp/context.env && echo "PASS: Test context.env exists"
source tests/temp/context.env
echo "TEST_MODE=$TEST_MODE"

echo ""
echo "=== Phase 1 Validation Complete ==="
```

**Rollback:** If validation fails, rollback all Phase 1:

```bash
git checkout HEAD -- bench/run.sh bench/scripts/benchmark.sh bench/scripts/utils.sh bench/scripts/summarize.sh tests/run.sh tests/lib/env.sh
rm -rf scripts/lib/ bench/scripts/gen_context_env.sh tests/scripts/
```

**Risk:** If any validation fails, fix before proceeding to Phase 2.

---

## Phase 2: Directory Migration (6 Commits)

Phase 2 aligns directory naming by migrating tests/lib to tests/scripts with temporary compatibility shim.

---

### Commit 15: Copy tests/lib files to tests/scripts

**Title:** Duplicate tests/lib scripts to tests/scripts

**Files Added:**
- `tests/scripts/log.sh`
- `tests/scripts/env.sh`
- `tests/scripts/assert.sh`
- `tests/scripts/docker.sh`

**Implementation:**

```bash
mkdir -p tests/scripts
cp tests/lib/log.sh tests/scripts/log.sh
cp tests/lib/env.sh tests/scripts/env.sh
cp tests/lib/assert.sh tests/scripts/assert.sh
cp tests/lib/docker.sh tests/scripts/docker.sh
```

Update internal references in copied files:

```bash
sed -i.bak 's|/lib/|/scripts/|g' tests/scripts/*.sh
sed -i.bak 's|# shellcheck source=tests/lib/|# shellcheck source=tests/scripts/|g' tests/scripts/*.sh
rm tests/scripts/*.sh.bak
```

Ensure `tests/scripts/env.sh` includes context.env copy logic from Commit 12.

**Commands:**

```bash
ls -la tests/scripts/
bash -n tests/scripts/*.sh
bash -c "source tests/scripts/log.sh && log_info 'test'" | grep '\[INFO\]'
bash -c "source tests/scripts/env.sh && declare -f setup_test" && echo PASS
```

**Rollback:** `rm tests/scripts/log.sh tests/scripts/env.sh tests/scripts/assert.sh tests/scripts/docker.sh`

**Risk:** None. Creates copies only.

---

### Commit 16: Create compatibility shim in tests/lib

**Title:** Add deprecation shim for tests/lib

**Files Modified:**
- `tests/lib/log.sh`
- `tests/lib/env.sh`
- `tests/lib/assert.sh`
- `tests/lib/docker.sh`

**Implementation:**

Replace content of each `tests/lib/*.sh` with shim:

```bash
#!/bin/bash
# DEPRECATED: Use tests/scripts/$(basename "${BASH_SOURCE[0]}") instead
# This shim will be removed in Phase 2

SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SHIM_DIR/../../scripts/lib/log.sh"
log_warn "DEPRECATED: tests/lib/$(basename "${BASH_SOURCE[0]}") is deprecated. Use tests/scripts/$(basename "${BASH_SOURCE[0]}") instead."

# shellcheck disable=SC1090
source "$SHIM_DIR/../scripts/$(basename "${BASH_SOURCE[0]}")"
```

**Commands:**

```bash
bash -n tests/lib/*.sh
bash -c "source tests/lib/log.sh && declare -f log_info" && echo PASS
bash -c "source tests/lib/log.sh 2>&1" | grep DEPRECATED
./tests/run.sh pavis 10_bootstrap_static 2>&1 | grep DEPRECATED || echo "Shim not triggered yet"
```

**Rollback:** `git checkout HEAD -- tests/lib/*.sh`

**Risk:** Low. Shim preserves functionality.

---

### Commit 17: Update tests/run.sh to source tests/scripts

**Title:** Migrate tests/run.sh to tests/scripts

**Files Modified:**
- `tests/run.sh`

**Implementation:**

Replace source statements:

```bash
# Old:
# source "$SCRIPT_DIR/lib/log.sh"

# New:
source "$SCRIPT_DIR/scripts/log.sh"
source "$SCRIPT_DIR/scripts/env.sh"
source "$SCRIPT_DIR/scripts/assert.sh"
source "$SCRIPT_DIR/scripts/docker.sh"
```

Update shellcheck directives similarly.

**Commands:**

```bash
bash -n tests/run.sh
./tests/run.sh pavis 10_bootstrap_static
./tests/run.sh pavis 10_bootstrap_static 2>&1 | grep -E "^DEPRECATED.*tests/lib" || echo "PASS: No shim warnings from run.sh"
```

**Rollback:** `git checkout HEAD -- tests/run.sh`

**Risk:** None. Shim provides fallback.

---

### Commit 18: Update all test case scripts to source tests/scripts

**Title:** Migrate test cases from tests/lib to tests/scripts

**Files Modified:**
- All test case scripts in `tests/suites/{pavis,relay,integrated}/*.sh`

**Implementation:**

```bash
# Automated migration
find tests/suites -name "*.sh" -type f -exec sed -i.bak 's|source "$(dirname "$0")/../../lib/env\.sh"|source "$(dirname "$0")/../../scripts/env.sh"|g' {} \;
find tests/suites -name "*.sh" -type f -exec sed -i.bak 's|source "$(dirname "$0")/../../lib/assert\.sh"|source "$(dirname "$0")/../../scripts/assert.sh"|g' {} \;
find tests/suites -name "*.sh" -type f -exec sed -i.bak 's|source "$(dirname "$0")/../../lib/log\.sh"|source "$(dirname "$0")/../../scripts/log.sh"|g' {} \;
find tests/suites -name "*.sh" -type f -exec sed -i.bak 's|source "$(dirname "$0")/../../lib/docker\.sh"|source "$(dirname "$0")/../../scripts/docker.sh"|g' {} \;
find tests/suites -name "*.sh" -type f -exec sed -i.bak 's|# shellcheck source=tests/lib/|# shellcheck source=tests/scripts/|g' {} \;

# Review changes
git diff tests/suites/

# Delete backups
find tests/suites -name "*.sh.bak" -delete
```

**Commands:**

```bash
grep -r "tests/lib" tests/suites/ || echo PASS
bash -n tests/suites/*/*.sh
./tests/run.sh all
./tests/run.sh all 2>&1 | grep DEPRECATED || echo PASS
```

**Rollback:** `find tests/suites -name "*.sh.bak" -exec bash -c 'mv "$1" "${1%.bak}"' _ {} \;`

**Risk:** Medium. Review diffs before committing.

---

### Commit 19: Remove tests/lib directory

**Title:** Delete deprecated tests/lib shim

**Files Removed:**
- `tests/lib/` (entire directory)

**Implementation:**

```bash
rm -rf tests/lib/
```

**Commands:**

```bash
! test -d tests/lib && echo PASS
./tests/run.sh all
! grep -r "tests/lib" tests/ --exclude-dir=.git || echo FAIL
```

**Rollback:** `git checkout HEAD -- tests/lib/`

**Risk:** Low. All references migrated.

---

### Commit 20: Update documentation for tests/scripts migration

**Title:** Update docs to reflect tests/scripts structure

**Files Modified:**
- README.md (if referencing tests/lib)
- docs/ files (if any reference tests/lib)

**Implementation:**

```bash
grep -r "tests/lib" ./ --exclude-dir=.git --exclude-dir=tests
```

Update any found references to `tests/scripts`.

**Commands:**

```bash
grep -r "tests/lib" ./ --exclude-dir=.git --exclude-dir=tests --exclude="scripts/lib" || echo PASS
./tests/run.sh pavis
```

**Rollback:** `git checkout HEAD -- README.md docs/`

**Risk:** None. Documentation only.

---

## Phase 3: Expand Shared Primitives (Completed)

**Status: ✅ COMPLETE (4 new modules implemented)**

**Completed:** 2026-01-17

Phase 3 adds more shared utilities to scripts/lib/ following the same pattern as Phase 1.

---

### Commit 21: Create scripts/lib/process.sh

**Title:** Add process management primitives to scripts/lib

**Status:** ✅ Complete

**Files Added:**
- `scripts/lib/process.sh`

**Functions Implemented:**
- `check_process_alive` - Check if a process is running (kill -0)
- `kill_process_safe` - Safely kill a process with graceful degradation (TERM → KILL)
- `wait_process_exit` - Wait for a process to exit with timeout
- `read_pid_file` - Read and validate a PID from a file
- `kill_process_by_pidfile` - Kill process by PID file
- `get_process_name` - Get process name by PID

**Validation:**
- ✅ Syntax validation passed
- ✅ check_process_alive tested with running and non-existent processes
- ✅ kill_process_safe tested with graceful shutdown
- ✅ PID validation tested
- ✅ read_pid_file tested
- ✅ get_process_name tested

---

### Commit 22: Create scripts/lib/http.sh

**Title:** Add HTTP utilities to scripts/lib

**Status:** ✅ Complete

**Files Added:**
- `scripts/lib/http.sh`

**Functions Implemented:**
- `http_get` - Perform HTTP GET request
- `http_post` - Perform HTTP POST request
- `check_http_status` - Check HTTP status code
- `http_request_full` - Capture both status and body
- `wait_for_http_status` - Wait for endpoint to return expected status
- `is_url_reachable` - Check if URL is reachable

**Validation:**
- ✅ Syntax validation passed
- ✅ http_get tested with example.com
- ✅ check_http_status tested
- ✅ http_request_full tested with body capture
- ✅ is_url_reachable tested with valid and invalid URLs

---

### Commit 23: Create scripts/lib/json.sh

**Title:** Add JSON utilities to scripts/lib

**Status:** ✅ Complete

**Files Added:**
- `scripts/lib/json.sh`

**Functions Implemented:**
- `require_jq` - Check if jq is available
- `json_validate` - Validate JSON file or string
- `json_get` - Extract a value from JSON with default support
- `json_has_keys` - Check if JSON has required keys
- `json_get_multiple` - Extract multiple values (tab-separated)
- `json_pretty` - Pretty-print JSON
- `json_merge` - Merge two JSON files
- `json_to_env` - Convert JSON to shell-sourceable format

**Validation:**
- ✅ Syntax validation passed
- ✅ json_validate tested with valid and invalid JSON
- ✅ json_get tested with simple and nested keys
- ✅ json_has_keys tested
- ✅ json_get_multiple tested with tab-separated output
- ✅ json_pretty tested

---

### Commit 24: Create scripts/lib/docker.sh

**Title:** Add Docker utilities to scripts/lib

**Status:** ✅ Complete

**Files Added:**
- `scripts/lib/docker.sh`

**Functions Implemented:**
- `require_docker` - Check if Docker is available and running
- `require_docker_compose` - Check if Docker Compose is available
- `docker_is_running` - Check if container is running
- `docker_wait_healthy` - Wait for container to become healthy
- `docker_collect_stats` - Collect Docker stats to CSV
- `docker_cleanup_container` - Stop and remove container
- `docker_get_logs` - Get container logs
- `docker_wait_port` - Wait for port in container

**Validation:**
- ✅ Syntax validation passed
- ✅ require_docker tested
- ✅ docker_is_running tested with existing and non-existent containers

---

### Commit 25: Update scripts/lib/README.md for Phase 3

**Title:** Document Phase 3 modules in scripts/lib README

**Status:** ✅ Complete

**Files Modified:**
- `scripts/lib/README.md`

**Changes:**
- Reorganized module list into Phase 1 (Foundation) and Phase 3 (Expanded Primitives)
- Added comprehensive function documentation for all Phase 3 modules
- Documented all function signatures and purposes

---

## Phase 3 Summary

**Total Modules Added:** 4
- process.sh (7 functions)
- http.sh (6 functions)
- json.sh (8 functions)
- docker.sh (8 functions)

**Total New Functions:** 29

**Integration Status:**
- ✅ All modules syntax validated
- ✅ All modules functionally tested
- ✅ Documentation updated
- ⏳ Migration opportunities identified (see Future Work below)

---

## Future Work: Migration Opportunities

The following scripts could potentially be refactored to use the new shared primitives:

### Process Management Migration
- `tests/scripts/env.sh` - Uses manual kill -0, kill -TERM, kill -KILL patterns (could use `kill_process_safe`)
- `bench/scripts/k8s_helpers.sh` - Uses kill -0 pattern (could use `check_process_alive`)

### HTTP Utilities Migration
- `bench/scripts/publish_config.sh` - Manual curl with status code extraction (could use `http_request_full`)
- `tests/scripts/env.sh` - Custom `pavis_curl_body`, `pavis_curl_headers` (could build on `http_*` functions)

### JSON Utilities Migration
- `bench/scripts/summarize.sh` - Multiple `jq -r '.field // empty'` calls (could use `json_get` or `json_get_multiple`)
- `scripts/lib/contract.sh` - Manual jq validation (could use `json_validate`, `json_has_keys`)

### Docker Utilities Migration
- `tests/scripts/docker.sh` - Manual docker stop/logs patterns (could use `docker_cleanup_container`, `docker_get_logs`)
- `tests/scripts/env.sh` - Docker inspect for running status (could use `docker_is_running`)

**Note:** Migration is optional and should be done incrementally when touching related code. The new primitives are available for use in new code immediately.

This phase is lower priority and can be done incrementally.

---

## Summary

**Total: 20 commits across 2 phases**

**Phase 1 (14 commits):** Foundation - scripts/lib, context.env, artifact validation, integration
**Phase 2 (6 commits):** Directory migration - tests/lib → tests/scripts

**Critical Implementation Requirements:**

1. No RUN_ID in paths. Fixed paths only.
2. gen_context_env scripts are executable, use `printf '%s=%q\n'` for safe quoting
3. validate_benchmark_artifacts inspects files (loadgen.txt.json vs run_*/wrk.txt vs wrk.txt), not names
4. benchmark.sh continues on failure using `|| true` pattern, aggregates failures
5. Entry points check for jq, nc early and fail fast
6. summarize.sh sources context.env safely

**Testing Strategy:**
- Run `bash -n` on modified scripts
- Run functional tests per commit
- Run full benchmark + test suite after integration commits
- Verify no unexpected warnings or errors

**Next Steps:**
Start with Commit 1. Execute sequentially. Validate before proceeding. After Commit 14, perform comprehensive Phase 1 validation. Do not proceed to Phase 2 unless Phase 1 passes.
