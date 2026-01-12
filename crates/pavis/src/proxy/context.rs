use pavis_core::{HeadersPolicy, Hostname, UpstreamName};
use std::sync::Arc;

pub struct RouterContext {
    pub upstream_name: Option<UpstreamName>,
    pub request_headers: Arc<HeadersPolicy>,
    pub response_headers: Arc<HeadersPolicy>,
    pub sni_override: Option<Hostname>,
    pub start_time: std::time::Instant,
    pub client_identity: Option<String>,
    pub rbac_denied: bool,
}

#[cfg(test)]
mod tests {
    use super::RouterContext;
    use pavis_core::{HeaderName, HeaderValue, Headers, HeadersPolicy, UpstreamName};
    use std::sync::Arc;

    #[test]
    fn router_context_holds_fields() {
        let ctx = RouterContext {
            upstream_name: Some(UpstreamName("backend".to_string())),
            request_headers: Arc::new(HeadersPolicy::Enabled {
                rules: Headers {
                    set_headers: vec![(
                        HeaderName("x-test".to_string()),
                        HeaderValue("1".to_string()),
                    )],
                    append_headers: Vec::new(),
                    add_headers: Vec::new(),
                    remove_headers: Vec::new(),
                },
            }),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: std::time::Instant::now(),
            client_identity: None,
            rbac_denied: false,
        };

        assert_eq!(
            ctx.upstream_name.as_ref().map(|v| v.0.as_str()),
            Some("backend")
        );
    }
}
