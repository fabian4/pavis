#!/bin/bash

# e2e/scripts/lib/fs.sh

ensure_tmp_dir() {
    local prefix="$1"
    local tmp_dir="$E2E_ROOT/tmp/${prefix}_$(date +%s%N)"
    mkdir -p "$tmp_dir"
    echo "$tmp_dir"
}
