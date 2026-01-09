# Audit Phase 2: Assertions & Oracles
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:10:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Assertion Inventory

The test suite relies on a mix of helper functions and ad-hoc shell commands for verification.

### Helper Library (`tests/lib/assert.sh`)
- `assert_body(url, expected)`: Performs a simple substring match on the HTTP response body.
- `assert_status(url, expected)`: exact match on the HTTP status code.

### Ad-Hoc Assertions
- **Header Verification:** Tests like `14_header_manipulation.sh` use `curl` combined with `grep` to verify headers are present/absent. This relies on the upstream backend echoing headers in the response body.
- **JSON Validation:** `04_observability.sh` uses bash string matching (`[[ "$RESP" == *"checksum"* ]]`) to verify JSON API responses.
- **Log Inspection:** Some failure cases (seen in Phase 0 exploration) check exit codes or log output implicitly via `grep`.

## 2. Oracle Quality

### External Behavior (Black-Box)
- **High Quality:** The majority of tests behave as true black-box clients. They assert on HTTP status codes, response bodies, and observable headers. This accurately reflects the user/operator perspective.
- **Decoupled:** Tests do not introspect the binary's internal memory or debug endpoints unless testing those specific endpoints (like `/v1/status`).

### Implementation Coupling
- **Low:** The assertions are largely decoupled from internal implementation. Changing the internal routing logic or config parsing would not break tests as long as the external HTTP behavior remains consistent.

## 3. Risks & Weaknesses

### Brittle JSON Parsing
- **Risk:** High. The use of substring matching for JSON (e.g., checking if "checksum" exists in the text) is fragile. It cannot distinguish between a key, a value, or a nested object. It does not validate the *structure* or *type* of the data.
- **Evidence:** `tests/suites/integrated/04_observability.sh` check for checksum presence.

### Upstream Dependency
- **Risk:** Medium. Header manipulation tests rely on the specific text format of the upstream echo server. If the upstream's response format changes (e.g., from `Key: Value` to `Key=Value`), tests will fail despite Pavis working correctly.

### False Positives
- **Risk:** Low-Medium. `grep` checks can match unintended strings. For example, ensuring a header is *removed* by grepping for it in the body might fail if the header name appears in the body for a different reason (e.g. part of the original request body echoed back).

## 4. Observations
- The lack of a proper JSON parser (like `jq`) in the test environment forces reliance on brittle string matching.
- Oracles are focused on "it works" rather than "it works correctly in detail" (e.g., checking that a checksum *exists*, not that it matches the config).
