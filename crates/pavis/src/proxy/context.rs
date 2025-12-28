use pavis_core::HeaderOperations;

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub start_time: std::time::Instant,
}
