#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

profile="${BENCH_PROFILE:-}"
if [[ -z "$profile" ]]; then
  case "${IS_CI:-${CI:-}}" in
    1|true|TRUE|yes|YES)
      profile="github"
      ;;
    *)
      profile="workstation"
      ;;
  esac
fi

if [[ "$profile" == "ci" ]]; then
  profile="github"
fi

case "$profile" in
  github)
    bash "${SCRIPT_DIR}/report_standalone_github.sh"
    ;;
  workstation)
    bash "${SCRIPT_DIR}/report_standalone_workstation.sh"
    ;;
  *)
    echo "error: unsupported BENCH_PROFILE=$profile (expected github or workstation)" >&2
    exit 1
    ;;
 esac
