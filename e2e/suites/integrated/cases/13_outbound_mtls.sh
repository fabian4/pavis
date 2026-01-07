#!/bin/bash
set -e

# e2e/suites/integrated/cases/13_outbound_mtls.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/integrated/docker-compose-integrated.yaml"

PORT_RELAY=8315
PORT_PAVIS=8100
PORT_BACKEND_TLS=8445

CASE_TMP=$(ensure_tmp_dir "integrated_13")

cleanup() {
    stop_pid "$CASE_TMP/backend.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/pavis.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/relay.pid" 2>/dev/null || true
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
    rm -f "$CASE_TMP/server_cert.pem" "$CASE_TMP/server_key.pem"
    rm -f "$CASE_TMP/client_cert.pem" "$CASE_TMP/client_key.pem"
    rm -f "$CASE_TMP/ca.pem"
}
trap cleanup EXIT

echo "⏭️  Skipping 13_outbound_mtls (requires mTLS client cert infrastructure)"
exit 0

# This test requires:
# - ClientCert::Enabled configuration for upstreams
# - Client certificate generation and validation
# - Mock TLS upstream that requires client certificates
# Currently skipped pending mTLS feature implementation
