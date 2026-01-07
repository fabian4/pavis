#!/bin/bash
set -e

# e2e/suites/relay/run.sh
SUITE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"

for case in "$SUITE_DIR/cases/"*.sh; do
    [ -e "$case" ] || continue
    echo "Running Relay Case: $(basename "$case")"

    # Clean up any previous compose stack
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true

    # Run the test case
    bash "$case"

    # Clean up after test
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
done

exit 0
