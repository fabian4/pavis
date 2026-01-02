#!/bin/bash
set -e

# Usage: TEST_MODE=binary|docker ./e2e-relay.sh
export TEST_MODE=${TEST_MODE:-binary}
export RELAY_IMAGE=${RELAY_IMAGE:-pavis-relay:local}

echo "🚀 Starting Relay E2E Test Suite in [$TEST_MODE] mode..."

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKSPACE_ROOT="$SCRIPT_DIR/../../.."

cleanup() {
    EXIT_CODE=$?
    if [ $EXIT_CODE -ne 0 ]; then
        echo "❌ Tests failed with exit code $EXIT_CODE"
        if [ "$TEST_MODE" == "docker" ]; then
            echo "📋 Dumping Docker logs for diagnostics..."
            # Relay tests use dynamic project names, we need to find them or use a pattern
            # For relay.rs, it uses pavis-relay-e2e-*
            for project in $(docker ps -a --format '{{.Label "com.docker.compose.project"}}' | grep pavis-relay-e2e- | sort -u); do
                echo "--- Logs for project: $project ---"
                docker compose -p "$project" logs --tail=500 || true
            done
        fi
    fi
    
    echo "🧹 Cleaning up..."
    CONFIG_DIR="$WORKSPACE_ROOT/crates/pavis-e2e/config"
    if [ "$TEST_MODE" == "docker" ]; then
        # Clean up temp files (use docker to handle root-owned files)
        docker run --rm -v "$CONFIG_DIR:/work" alpine rm -rf /work/relay_tmp 2>/dev/null || true
    fi
    rm -rf "$CONFIG_DIR"/relay_tmp 2>/dev/null || true
}
trap cleanup EXIT

ensure_binary() {
    if [ -f "$WORKSPACE_ROOT/target/release/pavis-relay" ]; then
        echo "✅ pavis-relay binary found at target/release/pavis-relay, skipping build."
    else
        echo "🚀 Building pavis-relay..."
        cd "$WORKSPACE_ROOT"
        cargo build -p pavis-relay --release
    fi
}

ensure_image() {
    if docker image inspect "$RELAY_IMAGE" > /dev/null 2>&1; then
        echo "✅ Relay image $RELAY_IMAGE found, skipping build."
    else
        echo "🚀 Building relay image $RELAY_IMAGE..."
        cd "$WORKSPACE_ROOT"
        docker build -f crates/pavis-relay/Dockerfile -t "$RELAY_IMAGE" .
    fi
}

if [ "$TEST_MODE" == "binary" ]; then
    ensure_binary
elif [ "$TEST_MODE" == "docker" ]; then
    ensure_image
else
    echo "❌ Unknown mode: $TEST_MODE. Use 'binary' or 'docker'."
    exit 1
fi

echo "🧪 Running Relay Tests via Rust Harness..."
cd "$WORKSPACE_ROOT"
cargo test -p pavis-e2e --test relay -- --test-threads=1 --nocapture

echo "🎉 Relay tests passed!"
