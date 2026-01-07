#!/bin/bash
set -e

# e2e/suites/integrated/cases/12_permissive_mtls.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/integrated/docker-compose-integrated.yaml"

PORT_RELAY=8314
PORT_PAVIS_TLS=8444
PORT_BACKEND=8099

CASE_TMP=$(ensure_tmp_dir "integrated_12")

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

echo "⏭️  Skipping 12_permissive_mtls (requires mTLS client cert infrastructure)"
exit 0

# This test requires:
# - ClientAuth::Optional configuration support
# - SPIFFE ID extraction from client certificates
# - Client certificate generation and validation
# Currently skipped pending mTLS feature implementation
