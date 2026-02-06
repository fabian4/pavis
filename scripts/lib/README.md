# scripts/lib - Shared Primitive Utilities

## Purpose

Shared primitive utilities for shell scripts across `bench/` and `tests/`.

## Critical Dependency Rule

**scripts/lib/ MUST NEVER source files from bench/ or tests/ to avoid circular dependencies.**

This directory provides foundational utilities that are sourced by higher-level scripts in bench/ and tests/, but never the reverse.

## Available Modules

### Phase 1: Foundation (Completed)

- **log.sh** - Logging functions
  - `log_info`, `log_warn`, `log_error`, `log_debug`, `log_section`, `exit_with_error`

- **time.sh** - Timestamp utilities
  - `timestamp_iso8601`, `timestamp_unix`, `timestamp_precise`, `duration_seconds`

- **contract.sh** - Artifact validation
  - `validate_benchmark_artifacts`, `validate_meta_json`, `validate_wrk_output`, `validate_loadgen_output`, `validate_docker_stats`, `require_cmd`

### Phase 3: Expanded Primitives (Completed)

- **process.sh** - Process management
  - `check_process_alive` - Check if a process is running
  - `kill_process_safe` - Safely kill a process with graceful degradation (TERM → KILL)
  - `wait_process_exit` - Wait for a process to exit
  - `read_pid_file` - Read and validate a PID from a file
  - `kill_process_by_pidfile` - Kill process by PID file
  - `get_process_name` - Get process name by PID

- **http.sh** - HTTP utilities
  - `http_get` - Perform HTTP GET request
  - `http_post` - Perform HTTP POST request
  - `check_http_status` - Check HTTP status code
  - `http_request_full` - Capture both status and body
  - `wait_for_http_status` - Wait for endpoint to return expected status
  - `is_url_reachable` - Check if URL is reachable

- **json.sh** - JSON utilities (jq wrappers)
  - `require_jq` - Check if jq is available
  - `json_validate` - Validate JSON file or string
  - `json_get` - Extract a value from JSON
  - `json_has_keys` - Check if JSON has required keys
  - `json_get_multiple` - Extract multiple values (tab-separated)
  - `json_pretty` - Pretty-print JSON
  - `json_merge` - Merge two JSON files
  - `json_to_env` - Convert JSON to shell-sourceable format

- **docker.sh** - Docker utilities
  - `require_docker` - Check if Docker is available and running
  - `require_docker_compose` - Check if Docker Compose is available
  - `docker_is_running` - Check if container is running
  - `docker_wait_healthy` - Wait for container to become healthy
  - `docker_collect_stats` - Collect Docker stats to CSV
  - `docker_cleanup_container` - Stop and remove container
  - `docker_get_logs` - Get container logs
  - `docker_wait_port` - Wait for port in container

## Usage

Source the desired module in your script:

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../scripts/lib/log.sh"

log_info "Starting process..."
log_debug "Debug info (only shown if DEBUG=1)"
```

## Design Principles

1. **Pure functions** - No global state mutation
2. **Explicit dependencies** - Clear sourcing, no implicit dependencies
3. **Fail fast** - Use `set -euo pipefail` in all scripts
4. **Self-contained** - Each module is independently testable
5. **Minimal dependencies** - Avoid external tools unless necessary

## Critical Design Decisions (Shell Ecosystem)

### Fixed Output Paths
**No RUN_ID in paths.** Outputs are cleaned before each run and remain at fixed locations:
- Bench run-scoped: `bench/output/{mode}/context.env`
- Bench case-scoped: `bench/output/{mode}/{proxy}/{case}/context.env`
- Tests run-scoped: `tests/temp/context.env`
- Tests case-scoped: `${TEST_TMP}/context.env`

### Context Generation Pattern
**Context generation scripts are executable.** They accept output path as `argv[1]`, write shell-sourceable key=value pairs using `printf '%s=%q\n'` for safe quoting, and exit non-zero on failure. Entry points invoke them via `bash path/to/gen_context_env.sh "$outfile"`, not by sourcing.

### File-Based Artifact Detection
**Artifact validation inspects actual files present, not case names.** `validate_benchmark_artifacts` checks if `loadgen.txt.json` exists (validates as loadgen), else checks for `run_*/wrk.txt` or `wrk.txt` (validates as wrk), and optionally validates `metrics.json` if present. Always validates `meta.json` schema.

### Resilient Execution
**Benchmark runner continues on failure.** The `run_case` function captures failures, marks failed cases with `.validation_failed`, and continues to next case. At end, exits 1 if any failures occurred.

### Explicit Dependencies
**Tool dependencies are explicit.** Entry points check for required commands (jq for validation, nc or bash /dev/tcp for port waits) and fail fast with clear messages if missing.

### Safe Configuration Loading
**Summarize script sources context.env safely.** It only reads RUN_* and BENCH_* variables; local variables use distinct names to avoid clobbering.
