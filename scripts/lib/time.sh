#!/bin/bash
set -euo pipefail

# Timestamp utilities for shell scripts

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
