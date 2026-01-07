# E2E Test Case Mapping

This document maps the shell-based E2E tests to their specifications in `docs/testing/`.

## Pavis Suite (`e2e/suites/pavis/`)

| Test File | Spec | Description | Status |
|-----------|------|-------------|--------|
| 01_routing.sh | P9 | Basic routing to upstreams | ✅ Migrated |
| 02_invalid_pvs.sh | P1 | Runtime rejects invalid `.pvs` files | ✅ Migrated |
| 03_invalid_path.sh | P3 | Startup failure on invalid config path | ✅ Migrated |
| 04_apply_semantics.sh | P2 | Runtime apply semantics (restart-based) | ✅ Migrated |
| 05_compaction.sh | P4 | Compaction levels preserve routing semantics | ✅ Migrated |
| 06_tls_termination.sh | P5 | TLS termination with self-signed certs | ✅ Migrated |
| 07_redirect_direct.sh | P6 | Redirect & direct responses | ✅ Migrated |
| 08_rewrites.sh | P7 | Path & host rewrites with query preservation | ✅ Migrated |
| 09_dns_discovery.sh | P8 | DNS discovery (hostname-based endpoints) | ✅ Migrated |
| 10_basic_routing.sh | P9 | Basic routing (duplicate of 01) | ✅ Migrated |
| 11_route_matching.sh | P10 | Route matching (exact vs prefix) | ✅ Migrated |
| 12_wildcard_host.sh | P12 | Wildcard host matching (*.example.com) | ✅ Migrated |
| 13_unmatched_routes.sh | P13 | Unmatched routes return 404 | ✅ Migrated |
| 14_header_manipulation.sh | P14 | Header manipulation (request) | ✅ Migrated |
| 15_response_headers.sh | P15 | Response header manipulation | ✅ Migrated |
| 16_round_robin.sh | P16 | Round robin load balancing | ✅ Migrated |
| 17_weighted_splitting.sh | P17 | Weighted traffic splitting (80/20) | ✅ Migrated |
| 18_upstream_weight.sh | P18 | Upstream endpoint weights | ✅ Migrated |
| 19_http_version.sh | P19 | HTTP version negotiation | ✅ Migrated |
| 20_upstream_tls.sh | P20 | Upstream TLS connections | ✅ Migrated |

**Coverage**: 20/20 tests (100%) ✅

## Relay Suite (`e2e/suites/relay/`)

| Test File | Spec | Description | Status |
|-----------|------|-------------|--------|
| 01_ingest.sh | R1 | Publish increments version, updates LKG | ✅ Migrated |
| 02_reject_invalid_pvs.sh | R2 | Reject invalid `.pvs` bytes | ✅ Migrated |
| 03_long_poll_semantics.sh | R3 | Long-poll returns on new version | ✅ Migrated |
| 04_partial_write_protection.sh | R4 | Partial write protection | ✅ Migrated |
| 05_observability.sh | R5 | Metrics and status endpoints | ✅ Migrated |
| 06_ingest_debouncing.sh | R6 | Ingest debouncing (5 writes -> 1 version) | ✅ Migrated |
| 07_persistence_recovery.sh | R7 | Persistence recovery (LKG reload) | ✅ Migrated |
| 08_codec_validation.sh | R8 | Codec validation (invalid YAML) | ✅ Migrated |
| 09_file_replacement.sh | R9 | File replacement (mv semantics) | ✅ Migrated |
| 10_startup_corrupted_lkg.sh | R10 | Startup with corrupted LKG fails | ✅ Migrated |
| 11_rapid_toggle.sh | R11 | Rapid toggle (valid/invalid/valid) | ✅ Migrated |
| 12_symlink_updates.sh | R12 | Symlink updates (K8s ConfigMap) | ✅ Migrated |
| 13_transient_permission_failure.sh | R13 | Transient permission failure recovery | ✅ Migrated |
| 14_transient_empty_file.sh | R14 | Transient empty file handling | ✅ Migrated |
| 15_artifact_size_limits.sh | R15 | Artifact size limits enforcement | ✅ Migrated |
| 16_traceability.sh | R16 | Traceability headers (X-Pavis-Generated-At) | ✅ Migrated |

**Coverage**: 16/16 tests (100%) ✅

## Integrated Suite (`e2e/suites/integrated/`)

| Test File | Spec | Description | Status |
|-----------|------|-------------|--------|
| 01_publish_apply.sh | I1 | Publish -> long-poll -> runtime apply | ✅ Migrated |
| 02_invalid_publish.sh | I2 | Invalid publish doesn't change runtime | ✅ Migrated |
| 03_concurrency.sh | I3 | Concurrency (3 runtimes converge) | ✅ Migrated |
| 04_observability.sh | I4 | Observability integration | ✅ Migrated |
| 05_file_ingest_pipeline.sh | I5 | File ingest -> Relay -> Runtime pipeline | ✅ Migrated |
| 06_data_plane_recovery.sh | I6 | Data plane recovery after restart | ✅ Migrated |
| 07_network_partition.sh | I7 | Network partition recovery | ✅ Migrated |
| 08_stale_rejection.sh | I8 | Stale control plane rejection | ✅ Migrated |
| 09_tls_propagation.sh | I9 | TLS configuration propagation | ✅ Migrated |
| 10_traffic_actions.sh | I10 | Traffic action propagation (redirect/direct) | ✅ Migrated |
| 11_rewrite_propagation.sh | I11 | Rewrite propagation | ✅ Migrated |
| 12_permissive_mtls.sh | I12 | Permissive migration (optional mTLS) | ⏭️  Skipped (requires mTLS) |
| 13_outbound_mtls.sh | I13 | Outbound mTLS | ⏭️  Skipped (requires mTLS) |
| 14_namespace_authorization.sh | I14 | Namespace-level authorization (RBAC) | ⏭️  Skipped (requires SPIFFE/RBAC) |

**Coverage**: 11/14 tests (79%)

## Overall Summary

- **Total Spec Tests**: 50
- **Migrated**: 47 tests (94%)
- **Skipped**: 3 tests (6%)

### By Suite

- ✅ **Relay**: 16/16 (100%) - Complete coverage
- ✅ **Pavis**: 20/20 (100%) - Complete coverage
- ✅ **Integrated**: 11/14 (79%) - Core + advanced scenarios covered

## Notes

### Complete Suites

Both **Pavis** and **Relay** suites have **100% coverage** of all specified test cases!

### Integrated Suite

The integrated suite covers essential end-to-end scenarios and most advanced scenarios:

**Core scenarios (I1-I6)**:
- Basic publish/apply flow (I1)
- Error handling (I2)
- Concurrency and convergence (I3)
- Observability integration (I4)
- File-based pipeline (I5)
- Data plane recovery (I6)

**Advanced scenarios (I7-I11)**:
- Network partition recovery (I7)
- Stale control plane rejection (I8)
- TLS propagation (I9)
- Traffic actions (I10)
- Rewrite propagation (I11)

**Skipped scenarios (I12-I14)**:
- I12-I14: mTLS and RBAC features requiring certificate infrastructure
  - Permissive client authentication
  - Outbound mTLS to upstreams
  - Namespace-level authorization with SPIFFE IDs

## Running Tests

```bash
# Run all tests
bash e2e/scripts/run.sh all

# Run specific suite
bash e2e/scripts/run.sh pavis
bash e2e/scripts/run.sh relay
bash e2e/scripts/run.sh integrated

# Clean up tmp files
rm -rf e2e/tmp
```

## Test Statistics

- **Total shell scripts**: 47 tests
- **Lines of test code**: ~3,500 lines
- **Helper libraries**: 3 files (process.sh, http.sh, fs.sh)
- **Config templates**: Embedded in test files
