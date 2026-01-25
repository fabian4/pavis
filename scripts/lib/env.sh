#!/usr/bin/env bash
set -euo pipefail

# Shared environment helpers.

resolve_bin() {
  # usage: resolve_bin <env_var_name> <bin_name> <fallback_path>
  local var="$1"
  local name="$2"
  local fallback="$3"

  # 1) explicit env wins
  local v="${!var:-}"
  if [[ -n "$v" ]]; then
    echo "$v"
    return 0
  fi

  # 2) PATH
  if command -v "$name" >/dev/null 2>&1; then
    command -v "$name"
    return 0
  fi

  # 3) repo fallback
  if [[ -x "$fallback" ]]; then
    echo "$fallback"
    return 0
  fi

  echo "ERROR: $name not found (env $var not set, not in PATH, fallback missing: $fallback)" >&2
  return 1
}
