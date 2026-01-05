#!/bin/bash
set -e

# Usage: TEST_MODE=binary|docker ./e2e-integrated.sh
export TEST_MODE=${TEST_MODE:-binary}
export RELAY_IMAGE=${RELAY_IMAGE:-pavis-relay:local}
export PAVIS_IMAGE=${PAVIS_IMAGE:-pavis:local}

echo "🚀 Starting Integrated E2E Test Suite in [$TEST_MODE] mode..."

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKSPACE_ROOT="$SCRIPT_DIR/../../.."
CONFIG_DIR="$WORKSPACE_ROOT/crates/pavis-e2e/config"
COMPOSE_FILE="$CONFIG_DIR/docker-compose-integrated.yaml"
PROJECT_NAME="pavis-e2e-$(date +%s%N)"
export RELAY_COMPOSE_FILE="$COMPOSE_FILE"
export PAVIS_COMPOSE_FILE="$COMPOSE_FILE"
export RELAY_COMPOSE_PROJECT="$PROJECT_NAME"
export PAVIS_COMPOSE_PROJECT="$PROJECT_NAME"
export PAVIS_RELAY_URL="http://relay:8080"
export RELAY_PORT=8083
export PAVIS_PORT=8084
export RELAY_WORK_DIR="$CONFIG_DIR/relay_tmp"
export PAVIS_WORK_DIR="$CONFIG_DIR/pavis_tmp"

# Create work directories
mkdir -p "$RELAY_WORK_DIR" "$PAVIS_WORK_DIR"

cleanup() {
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "❌ Tests failed with exit code $EXIT_CODE"
    if [ "$TEST_MODE" == "docker" ]; then
      echo "📋 Dumping Docker logs for diagnostics..."
      docker compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" logs --tail=500 || true
    fi
  fi

  echo "🧹 Cleaning up..."
  if [ "$TEST_MODE" == "docker" ]; then
    docker compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" down > /dev/null 2>&1 || true
    # Clean up temp files (use docker to handle root-owned files)
    docker run --rm -v "$CONFIG_DIR:/work" alpine rm -rf /work/relay_tmp /work/pavis_tmp 2>/dev/null || true
  fi
  rm -rf "$CONFIG_DIR"/relay_tmp "$CONFIG_DIR"/pavis_tmp 2>/dev/null || true
}
trap cleanup EXIT

ensure_binary() {
    if [ "$TEST_MODE" == "binary" ]; then
        if [ -f "$WORKSPACE_ROOT/target/release/pavis" ]; then
            echo "✅ Pavis binary found at target/release/pavis, skipping build."
        else
            echo "🚀 Building pavis..."
            cd "$WORKSPACE_ROOT"
            cargo build -p pavis --release
        fi

        if [ -f "$WORKSPACE_ROOT/target/release/pavis-relay" ]; then
            echo "✅ pavis-relay binary found at target/release/pavis-relay, skipping build."
        else
            echo "🚀 Building pavis-relay..."
            cd "$WORKSPACE_ROOT"
            cargo build -p pavis-relay --release
        fi
    fi
}

ensure_images() {
    if [ "$TEST_MODE" == "docker" ]; then
        if docker image inspect "$PAVIS_IMAGE" > /dev/null 2>&1; then
            echo "✅ Pavis image $PAVIS_IMAGE found, skipping build."
        else
            echo "🚀 Building pavis image $PAVIS_IMAGE..."
            cd "$WORKSPACE_ROOT"
            docker build -f crates/pavis/Dockerfile -t "$PAVIS_IMAGE" .
        fi

        if docker image inspect "$RELAY_IMAGE" > /dev/null 2>&1; then
            echo "✅ Relay image $RELAY_IMAGE found, skipping build."
        else
            echo "🚀 Building relay image $RELAY_IMAGE..."
            cd "$WORKSPACE_ROOT"
            docker build -f crates/pavis-relay/Dockerfile -t "$RELAY_IMAGE" .
        fi
    fi
}

start_backends() {
    if [ "$TEST_MODE" == "docker" ]; then
        echo "🐳 Starting Upstreams (Backends only)..."
        docker compose -f "$COMPOSE_FILE" -p "$PROJECT_NAME" up -d backend-v1 backend-v2

        echo "⏳ Waiting for backends to be ready..."
        MAX_RETRIES=10
        count=0
        until curl -s -o /dev/null http://localhost:8081 || [ $count -eq $MAX_RETRIES ]; do
          echo -n "."
          sleep 1
          count=$((count+1))
        done
        echo ""

        if [ $count -eq $MAX_RETRIES ]; then
            echo "❌ Timeout waiting for backend-v1"
            exit 1
        fi
    fi
}

if [ "$TEST_MODE" != "binary" ] && [ "$TEST_MODE" != "docker" ]; then
    echo "❌ Unknown mode: $TEST_MODE. Use 'binary' or 'docker'."
    exit 1
fi

ensure_binary
ensure_images
start_backends

echo "🧪 Running Integrated Tests via Rust Harness..."
cd "$WORKSPACE_ROOT"
cargo test -p pavis-e2e --test integrated -- --test-threads=1 --nocapture
cargo test -p pavis-e2e --test chaos_reloads -- --test-threads=1 --nocapture

echo "🎉 Integrated tests passed!"
