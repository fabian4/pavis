#!/bin/bash
set -e

# Usage: TEST_MODE=binary|docker ./e2e.sh
export TEST_MODE=${TEST_MODE:-binary} # Default to binary if not set

echo "🚀 Starting E2E Test Suite in [$TEST_MODE] mode..."

# Define paths (relative to workspace root)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKSPACE_ROOT="$SCRIPT_DIR/.."
CONFIG_DIR="$WORKSPACE_ROOT/crates/pavis-e2e/config"
COMPOSE_FILE="$CONFIG_DIR/docker-compose.yaml"

# Cleanup function
cleanup() {
  echo "🧹 Cleaning up..."
  # Stop backends
  docker compose -f "$COMPOSE_FILE" down > /dev/null 2>&1 || true
  # Cleanup generated configs logic handled by Rust TestEnv drop, 
  # but we can do a sweep here just in case of panic aborts.
  rm -f "$CONFIG_DIR"/generated_*.yaml
}
trap cleanup EXIT

# 1. Setup Environment Variables
if [ "$TEST_MODE" == "docker" ]; then
    export BACKEND_V1_HOST="backend-v1"
    export BACKEND_V2_HOST="backend-v2"
    # Ensure the pavis image is built/available if using docker mode
    # Assuming CI or makefile did this.
    
elif [ "$TEST_MODE" == "binary" ]; then
    export BACKEND_V1_HOST="127.0.0.1"
    export BACKEND_V2_HOST="127.0.0.1"
else
    echo "❌ Unknown mode: $TEST_MODE. Use 'binary' or 'docker'."
    exit 1
fi

# 2. Start Infrastructure (Backends)
echo "🐳 Starting Upstreams (Backends only)..."
docker compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2

# Wait for backends
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

if [ "$TEST_MODE" == "binary" ]; then
    echo "🚀 Building Pavis Binary..."
    cd "$WORKSPACE_ROOT"
    cargo build -p pavis --release
fi

# 3. Run Tests
echo "🧪 Running Tests via Rust Harness..."
cd "$WORKSPACE_ROOT"

# We MUST run sequentially because we share port 8080 / the single docker container.
cargo test -p pavis-e2e -- --test-threads=1 --nocapture

echo "🎉 All tests passed!"
