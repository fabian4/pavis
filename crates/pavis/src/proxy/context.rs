use pavis_core::HeaderOperations;

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub start_time: std::time::Instant,
}

#[cfg(test)]
mod tests {
    use super::RouterContext;
    use pavis_core::HeaderOperations;

    #[test]
    fn router_context_holds_fields() {
        let ctx = RouterContext {
            upstream_name: Some("backend".to_string()),
            request_headers: Some(HeaderOperations {
                add: vec![("x-test".to_string(), "1".to_string())],
                remove: vec![],
            }),
            response_headers: None,
            start_time: std::time::Instant::now(),
        };

        assert_eq!(ctx.upstream_name.as_deref(), Some("backend"));
        assert!(ctx.request_headers.is_some());
        assert!(ctx.response_headers.is_none());
    }
}
