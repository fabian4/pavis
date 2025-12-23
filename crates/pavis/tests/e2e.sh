#!/bin/bash
set -e

# Usage: TEST_MODE=binary|docker ./e2e.sh
export TEST_MODE=${TEST_MODE:-binary} # Default to binary if not set

echo "🚀 Starting E2E Test Suite in [$TEST_MODE] mode..."

# Define paths
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
CONFIG_TPL="$SCRIPT_DIR/config.yaml"
CONFIG_OUT="$SCRIPT_DIR/generated_config.yaml"

# Cleanup function
cleanup() {
  echo "🧹 Cleaning up..."
  if [ -f pavis.pid ]; then
    echo "Killing local Pavis process..."
    kill $(cat pavis.pid) || true
    rm pavis.pid
  fi
  
  echo "Stopping Docker containers..."
  cd "$SCRIPT_DIR" && docker compose down > /dev/null 2>&1 || true
  
  # Remove generated config
  rm -f "$CONFIG_OUT"
}
trap cleanup EXIT

# 1. Setup Environment Variables & Config
if [ "$TEST_MODE" == "docker" ]; then
    # In Docker, Pavis talks to other containers by name
    export BACKEND_V1_HOST="backend-v1"
    export BACKEND_V2_HOST="backend-v2"
    
elif [ "$TEST_MODE" == "binary" ]; then
    # In Binary, Pavis talks to localhost
    export BACKEND_V1_HOST="127.0.0.1"
    export BACKEND_V2_HOST="127.0.0.1"
else
    echo "❌ Unknown mode: $TEST_MODE. Use 'binary' or 'docker'."
    exit 1
fi

echo "📝 Generating config from template..."
sed -e "s|\${BACKEND_V1_HOST}|$BACKEND_V1_HOST|g" \
    -e "s|\${BACKEND_V2_HOST}|$BACKEND_V2_HOST|g" \
    -e "s|\${TEST_MODE}|$TEST_MODE|g" \
    "$CONFIG_TPL" > "$CONFIG_OUT"

# 2. Start Infrastructure
if [ "$TEST_MODE" == "docker" ]; then
    echo "🐳 Building and Starting Full Stack (Pavis + Backends)..."
    cd "$SCRIPT_DIR"
    # We need to ensure the pavis container sees the generated config.
    # The docker-compose mounts ./generated_config.yaml:/etc/pavis/config.yaml
    docker compose up -d
    
elif [ "$TEST_MODE" == "binary" ]; then
    echo "🐳 Starting Upstreams (Backends only)..."
    cd "$SCRIPT_DIR"
    docker compose up -d backend-v1 backend-v2

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

    echo "🚀 Starting Pavis Binary..."
    cd ../../.. # Go to root
    
    PAVIS_BIN="./target/release/pavis"
    if [ ! -f "$PAVIS_BIN" ]; then
        echo "🦀 Pavis binary not found, building..."
        cargo build -p pavis --release
    fi
    
    # Run in background with generated config
    $PAVIS_BIN --config "$CONFIG_OUT" &
    echo $! > pavis.pid
    
    echo "⏳ Giving Pavis a moment to initialize..."
    sleep 2
fi

# 3. Run Tests
echo "🧪 Delegating to shared verification script..."
# Ensure we are in the root or correct relative path for the test script if it relies on it
# The test script is in crates/pavis/tests/test.sh.
# We are currently in either $SCRIPT_DIR or root depending on the block above.
# Let's standardize to root.
cd "$SCRIPT_DIR/../../.."

bash crates/pavis/tests/test.sh