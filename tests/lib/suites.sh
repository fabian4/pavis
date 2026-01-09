#!/bin/bash

# tests/lib/suites.sh

UPSTREAMS_COMPOSE="$PROJECT_ROOT/tests/config/upstreams.yaml"
CERTS_DIR="$PROJECT_ROOT/tests/config/certs"

generate_certs() {
    echo "🔑 Generating upstream certificates..."
    mkdir -p "$CERTS_DIR"
    openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout "$CERTS_DIR/upstream_tls.key" \
        -out "$CERTS_DIR/upstream_tls.pem" \
        -subj "/CN=localhost" -days 365 2>/dev/null
}

cleanup_certs() {
    echo "🧹 Cleaning up upstream certificates..."
    rm -rf "$CERTS_DIR"
}

start_upstreams() {
    generate_certs
    echo "🚀 Starting shared upstreams..."
    docker compose -f "$UPSTREAMS_COMPOSE" up -d --wait
}

stop_upstreams() {
    echo "🛑 Stopping shared upstreams..."
    docker compose -f "$UPSTREAMS_COMPOSE" down -v
    cleanup_certs
}
