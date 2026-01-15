#!/bin/bash
set -e

# Case: security_06_tls_sni_auto
# Category: Security & TLS
# Description: Verifies Auto SNI derivation and fail-fast for invalid Auto SNI configs.

# SKIP: Pingora's rustls connector does not support per-peer CA certificates yet.
# See: https://github.com/cloudflare/pingora/blob/main/pingora-core/src/connectors/tls/rustls/mod.rs
# TODO: Re-enable when pingora implements per-peer CA support or when switching to OpenSSL backend
echo "⏭️ SKIPPED: Pingora rustls does not support per-peer CA certificates"
exit 0

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_06"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    discovery: "logical"
    tls:
      enabled: true
      verify_cert: true
      verify_hostname: true
      sni_mode: auto
      ca_bundle_path: "$CERTS_DIR/ca.pem"
    endpoints:
      - address: "localhost"
        port: ${UPSTREAM_HTTPS_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" ""

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 10
assert_status "http://127.0.0.1:$PORT_PAVIS/healthz" "200"

cat <<EOF > "$TEST_TMP/invalid.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    tls:
      enabled: true
      verify_cert: true
      verify_hostname: true
      sni_mode: auto
    endpoints:
      - address: "127.0.0.1"
        port: ${UPSTREAM_HTTPS_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF

if "$PAVCTL_BIN" gen "$TEST_TMP/invalid.yaml" "$TEST_TMP/invalid.pvs" >"$TEST_TMP/logs/gen_invalid.log" 2>&1; then
  echo "❌ Expected config validation failure for Auto SNI with IP endpoint"
  exit 1
fi

if ! grep -q "verify=full with sni=auto requires DNS endpoints or route host rewrite" "$TEST_TMP/logs/gen_invalid.log"; then
  echo "❌ Expected validation error message for Auto SNI with IP endpoint"
  exit 1
fi

echo "✅ security_06_tls_sni_auto passed"
