# Test Case Review Report

**Date:** 2025-12-28
**Scope:** `crates/pavis-core`, `crates/pavis`, `crates/pavis-pvs`, `crates/pavis-adapter-yaml`, `crates/pavctl`

## 1. Test Coverage
*   **Strengths:**
    *   **Core Protocol:** `pavis-core` validation logic is extensively tested, covering valid/invalid inputs for upstreams, routes, headers, and regexes.
    *   **Runtime Logic:** `pavis` proxy logic tests (routing, weighted load balancing, header manipulation) are robust and verify correctness.
    *   **Binary Lifecycle:** `pavis-pvs` correctly tests header parsing and valid payload verification.
*   **Gaps:**
    *   **pavis (Integration Tests)**:
        *   **README Parity**: The `tests/README.md` claims coverage for `--version` and "Configuration-Driven Routing", but code lacks version tests and only covers Prefix matching (missing Exact and Regex at integration level).
        *   **VHost Precedence**: No tests for multiple specific hosts (e.g., `api.com` vs `*`) to verify selection logic.
        *   **Header Propagation**: No test verifying that headers defined in `RuntimeConfig` correctly populate the `RouterContext`.
        *   **Cluster Edge Cases**: Missing tests for upstreams with **zero endpoints**.
    *   **pavis-adapter-yaml (UX)**:
        *   **Field Strictness**: No tests for `deny_unknown_fields` behavior.
        *   **Malformed Durations**: Missing tests for invalid humantime strings (e.g., `-5s`).
    *   **pavis-pvs (Integrity)**:
        *   **Truncated Files**: Verifies `TooSmall` for headers, but lacks tests for files truncated mid-payload.
        *   **Invalid Magic**: No test for a valid-sized file with incorrect magic bytes (e.g., a text file renamed to `.pvs`).

## 2. Test Structure
*   **Naming:** Generally clear (e.g., `test_find_route_regex_match`, `test_upstream_tls`).
*   **AAA Pattern:** Consistently followed in unit tests.
*   **Stability:** `checksum.rs` and `cli.rs` use `std::thread::sleep` for process synchronization. This is brittle; recommended to move toward port-probing or log-watching.

## 3. Test Maintenance
*   **Isolation:** Unit tests are pure and parallel-safe. Integration tests use temporary directories and unique ports.
*   **Redundancy**: `checksum.rs` and `cli.rs` overlap significantly in "black-box" binary testing and could be consolidated.

## 4. Error Handling
*   **Coverage:** Validation tests explicitly check for error variants (`matches!(err, CoreValidationError::...)`).
*   **Simulation:** `checksum.rs` effectively simulates file corruption (bit flipping) to verify integrity checks.

## 5. Performance
*   **Speed:** Unit tests are instantaneous.
*   **Scalability**: No tests currently verify behavior with extremely large configs (e.g., 10,000+ routes).

## 6. Summary & Recommendations

The test suite is "logic-complete" for happy paths but "boundary-incomplete" for operational edge cases.

### Priority Recommendation Table

| Crate | Priority | Missing Test Case |
| :--- | :--- | :--- |
| `pavis` | **High** | Host header port stripping (`example.com:80`) |
| `pavis` | **High** | Regex Integration (verify PVS -> Router -> Match) |
| `pavis` | **Medium** | CLI `--version` check (matches README) |
| `pavis-core` | **High** | Route overlap/conflict detection |
| `pavis-pvs` | **Medium** | Truncated payload integrity & Invalid Magic |
| `pavis-adapter` | **Low** | Strictness of unknown YAML fields |

**Action Plan:** Address the "README vs Implementation" gaps first (Version flag and Regex integration) to ensure documented features are actually verified.