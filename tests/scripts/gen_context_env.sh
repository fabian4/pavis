#!/usr/bin/env bash
set -euo pipefail

# Generate shell-sourceable context.env for test runs
# Usage: ./gen_context_env.sh <output_file>
# Output format: KEY=value (safely quoted with printf '%s=%q\n')

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/log.sh
source "$SCRIPT_DIR/../../scripts/lib/log.sh"
# shellcheck source=scripts/lib/time.sh
source "$SCRIPT_DIR/../../scripts/lib/time.sh"

main() {
  local output_file="${1:-}"

  if [[ -z "$output_file" ]]; then
    log_error "Usage: $0 <output_file>"
    exit 1
  fi

  local project_root
  project_root="${PROJECT_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
  local tests_root
  tests_root="$(cd "$SCRIPT_DIR/.." && pwd)"

  mkdir -p "$(dirname "$output_file")"

  local run_timestamp
  run_timestamp="$(timestamp_iso8601)"
  local git_sha
  git_sha="$(git -C "$project_root" rev-parse HEAD 2>/dev/null || echo "unknown")"
  local run_tag
  run_tag="$(git -C "$project_root" rev-parse --short HEAD 2>/dev/null || echo "unknown")"

  local test_mode="${TEST_MODE:-binary}"
  local test_suite="${TEST_SUITE:-}"

  local pavis_bin="${PAVIS_BIN:-$project_root/target/release/pavis}"
  local relay_bin="${RELAY_BIN:-$project_root/target/release/pavis-relay}"
  local pavctl_bin="${PAVCTL_BIN:-$project_root/target/release/pavctl}"
  local pavis_upstream_bin="${PAVIS_UPSTREAM_BIN:-$project_root/target/release/pavis-mock-upstream}"
  local mock_relay_bin="${MOCK_RELAY_BIN:-$project_root/target/release/pavis-mock-relay}"

  local pavis_image="${PAVIS_IMAGE:-pavis:local}"
  local relay_image="${RELAY_IMAGE:-pavis-relay:local}"
  local mock_relay_image="${MOCK_RELAY_IMAGE:-pavis-mock-relay:local}"

  local upstream_http_port_v1="${UPSTREAM_HTTP_PORT_V1:-8081}"
  local upstream_http_port_v2="${UPSTREAM_HTTP_PORT_V2:-8082}"
  local upstream_https_port_v1="${UPSTREAM_HTTPS_PORT_V1:-8443}"
  local upstream_https_port_v2="${UPSTREAM_HTTPS_PORT_V2:-8444}"

  local e2e_verbose="${E2E_VERBOSE:-0}"
  local e2e_parallel="${E2E_PARALLEL:-0}"
  local e2e_filter="${E2E_FILTER:-}"
  local skip_cleanup="${SKIP_CLEANUP:-0}"

  local test_scripts_dir="${TEST_SCRIPTS_DIR:-$tests_root/scripts}"

  {
    printf '%s=%q\n' "RUN_TIMESTAMP" "$run_timestamp"
    printf '%s=%q\n' "GIT_SHA" "$git_sha"
    printf '%s=%q\n' "RUN_TAG" "$run_tag"

    printf '%s=%q\n' "TEST_MODE" "$test_mode"
    printf '%s=%q\n' "TEST_SUITE" "$test_suite"

    printf '%s=%q\n' "PAVIS_BIN" "$pavis_bin"
    printf '%s=%q\n' "RELAY_BIN" "$relay_bin"
    printf '%s=%q\n' "PAVCTL_BIN" "$pavctl_bin"
    printf '%s=%q\n' "PAVIS_UPSTREAM_BIN" "$pavis_upstream_bin"
    printf '%s=%q\n' "MOCK_RELAY_BIN" "$mock_relay_bin"

    printf '%s=%q\n' "PAVIS_IMAGE" "$pavis_image"
    printf '%s=%q\n' "RELAY_IMAGE" "$relay_image"
    printf '%s=%q\n' "MOCK_RELAY_IMAGE" "$mock_relay_image"

    printf '%s=%q\n' "UPSTREAM_HTTP_PORT_V1" "$upstream_http_port_v1"
    printf '%s=%q\n' "UPSTREAM_HTTP_PORT_V2" "$upstream_http_port_v2"
    printf '%s=%q\n' "UPSTREAM_HTTPS_PORT_V1" "$upstream_https_port_v1"
    printf '%s=%q\n' "UPSTREAM_HTTPS_PORT_V2" "$upstream_https_port_v2"

    printf '%s=%q\n' "E2E_VERBOSE" "$e2e_verbose"
    printf '%s=%q\n' "E2E_PARALLEL" "$e2e_parallel"
    printf '%s=%q\n' "E2E_FILTER" "$e2e_filter"
    printf '%s=%q\n' "SKIP_CLEANUP" "$skip_cleanup"

    printf '%s=%q\n' "PROJECT_ROOT" "$project_root"
    printf '%s=%q\n' "SCRIPT_DIR" "$tests_root"
    printf '%s=%q\n' "TEST_SCRIPTS_DIR" "$test_scripts_dir"
  } > "$output_file"

  log_info "Generated test context: $output_file"
}

main "$@"
