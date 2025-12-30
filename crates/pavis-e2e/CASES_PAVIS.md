# Pavis E2E Cases

Source: `crates/pavis-e2e/tests/pavis/*.rs`

## Basic routing
- Test: `crates/pavis-e2e/tests/pavis/basic_routing.rs`
- Scenario: `PavisScenario::BasicRouting`
- Setup: two upstreams (`backend-v1`, `backend-v2`) with round-robin load balancer.
- Request: GET `/` multiple times.
- Assert: responses identify either backend and never an unknown upstream.

## Header manipulation (request)
- Test: `crates/pavis-e2e/tests/pavis/header_manipulation.rs`
- Scenario: `PavisScenario::HeaderManipulation`
- Setup: route `/headers` adds `X-Pavis-Added`, `X-Multi-Word`; removes `X-Pavis-Remove-Me`.
- Request: GET `/headers` with `X-Pavis-Remove-Me` and `X-Keep-Me`.
- Assert: added headers are present, removed header is absent, `X-Keep-Me` is preserved.

## HTTP version config
- Test: `crates/pavis-e2e/tests/pavis/http_version.rs`
- Scenario: `PavisScenario::HttpVersion`
- Setup: upstream-h1 uses H1, upstream-h2 uses H2H1.
- Request: GET `/h1/test`, GET `/h2/test`.
- Assert: `/h1` routes to backend-v1, `/h2` routes to backend-v2.

## Regex matching
- Test: `crates/pavis-e2e/tests/pavis/regex_matching.rs`
- Scenario: `PavisScenario::RegexMatching`
- Setup: regex routes for `/api/vN/users/<id>` and `/posts/<slug>`, fallback prefix `/`.
- Requests:
  - `/api/v1/users/123` (regex match) -> backend-v1
  - `/api/v2/users/456` (regex match) -> backend-v1
  - `/api/v1/users/abc` (regex miss) -> fallback -> backend-v1
  - `/posts/hello-world` (regex match) -> backend-v2
  - `/posts/my-first-post-2024` (regex match) -> backend-v2
  - `/posts/Hello_World` (regex miss) -> fallback -> backend-v1
- Assert: matches and fallbacks route to the expected upstreams.

## Response header manipulation
- Test: `crates/pavis-e2e/tests/pavis/response_headers.rs`
- Scenario: `PavisScenario::ResponseHeaders`
- Setup: host `response-headers` adds response headers and removes `Server`.
- Request: GET `/headers` with Host `response-headers`.
- Assert: response includes added headers and excludes `Server`.

## Round-robin balancing
- Test: `crates/pavis-e2e/tests/pavis/round_robin.rs`
- Scenario: `PavisScenario::RoundRobin`
- Setup: mixed upstream with two endpoints.
- Request: repeated GET `/round-robin`.
- Assert: upstream alternates across successive requests.

## Route matching precedence
- Test: `crates/pavis-e2e/tests/pavis/route_matching.rs`
- Scenario: `PavisScenario::RouteMatching`
- Setup: exact `/exact-only`, prefix `/prefix-match`, fallback `/`.
- Requests:
  - `/exact-only` -> backend-v1
  - `/exact-only/something` -> fallback `/` -> backend-v1
  - `/prefix-match` -> backend-v2
  - `/prefix-match/anything` -> backend-v2
- Assert: exact and prefix matches take precedence over fallback.

## TLS listener support
- Test: `crates/pavis-e2e/tests/pavis/tls_support.rs`
- Scenario: built via `tls_support_config`
- Setup: HTTPS listener at `:8443` with self-signed cert; upstream over HTTP.
- Request: GET `https://localhost:8443/` (client accepts invalid certs).
- Assert: response succeeds and contains backend identity.

## Unmatched routes
- Test: `crates/pavis-e2e/tests/pavis/unmatched_routes.rs`
- Scenario: `PavisScenario::UnmatchedRoutes`
- Setup: only host `example.com` with prefix `/api`.
- Requests:
  - `/nonexistent` -> 404
  - Host `wrong-host.com` + `/api/test` -> 404
  - Host `example.com` + `/api/test` -> 200
- Assert: unmatched path/host returns 404, correct host/path succeeds.

## Upstream TLS
- Test: `crates/pavis-e2e/tests/pavis/upstream_tls.rs`
- Scenario: built via `upstream_tls_config`
- Setup: TLS backend with self-signed cert; upstream TLS verification disabled.
- Request: GET `/`.
- Assert: response succeeds through TLS upstream.

## Upstream weight
- Test: `crates/pavis-e2e/tests/pavis/upstream_weight.rs`
- Scenario: `PavisScenario::UpstreamWeight`
- Setup: weighted endpoints (v1 weight 3, v2 weight 1).
- Request: repeated GET `/`.
- Assert: v1 responses significantly outnumber v2 responses.

## Weighted splitting
- Test: `crates/pavis-e2e/tests/pavis/weighted_splitting.rs`
- Scenario: `PavisScenario::WeightedSplitting`
- Setup: route `/weighted-test` splits 80/20 between v1 and v2.
- Request: repeated GET `/weighted-test`.
- Assert: v1 responses dominate with expected skew.

## Wildcard host routing
- Test: `crates/pavis-e2e/tests/pavis/wildcard_host.rs`
- Scenario: `PavisScenario::WildcardHost`
- Setup: exact host `api.example.com` routes to v1; wildcard `*` routes to v2.
- Requests:
  - Host `api.example.com` -> v1
  - Host `other.example.com` -> v2
  - Host `random-host.io` -> v2
  - Host `localhost` -> v2
- Assert: exact host wins; wildcard catches everything else.
