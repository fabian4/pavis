#!/bin/bash
set -e

# e2e/suites/integrated/cases/14_namespace_authorization.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/integrated/docker-compose-integrated.yaml"

PORT_RELAY=8316
PORT_PAVIS_TLS=8446
PORT_BACKEND=8101

CASE_TMP=$(ensure_tmp_dir "integrated_14")

cleanup() {
    stop_pid "$CASE_TMP/backend.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/pavis.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/relay.pid" 2>/dev/null || true
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
    rm -f "$CASE_TMP/server_cert.pem" "$CASE_TMP/server_key.pem"
    rm -f "$CASE_TMP/prod_cert.pem" "$CASE_TMP/prod_key.pem"
    rm -f "$CASE_TMP/dev_cert.pem" "$CASE_TMP/dev_key.pem"
    rm -f "$CASE_TMP/ca.pem"
}
trap cleanup EXIT

echo "⏭️  Skipping 14_namespace_authorization (requires SPIFFE/RBAC infrastructure)"
exit 0

# This test requires:
# - Principal::Prefix configuration support
# - SPIFFE ID extraction and validation
# - Client certificates with SPIFFE IDs in SAN
# - RBAC policy enforcement
# Currently skipped pending SPIFFE/RBAC feature implementation
