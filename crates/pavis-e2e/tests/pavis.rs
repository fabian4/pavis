#[path = "pavis/basic_routing.rs"]
mod basic_routing;
#[path = "pavis/common/mod.rs"]
mod common;
#[path = "pavis/header_manipulation.rs"]
mod header_manipulation;
#[path = "pavis/http_version.rs"]
mod http_version;
#[path = "pavis/regex_matching.rs"]
mod regex_matching;
#[path = "pavis/response_headers.rs"]
mod response_headers;
#[path = "pavis/round_robin.rs"]
mod round_robin;
#[path = "pavis/route_matching.rs"]
mod route_matching;
#[path = "pavis/tls_support.rs"]
mod tls_support;
#[path = "pavis/unmatched_routes.rs"]
mod unmatched_routes;
#[path = "pavis/upstream_tls.rs"]
mod upstream_tls;
#[path = "pavis/upstream_weight.rs"]
mod upstream_weight;
#[path = "pavis/weighted_splitting.rs"]
mod weighted_splitting;
#[path = "pavis/wildcard_host.rs"]
mod wildcard_host;
