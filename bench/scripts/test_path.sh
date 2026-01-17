#!/bin/bash
UTILS_DIR_ABS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "UTILS_DIR_ABS=$UTILS_DIR_ABS"
BENCH_SCRIPTS_ROOT="$(cd "$UTILS_DIR_ABS/../.." && pwd)"
echo "BENCH_SCRIPTS_ROOT=$BENCH_SCRIPTS_ROOT"
echo "Looking for: $BENCH_SCRIPTS_ROOT/scripts/lib/log.sh"
