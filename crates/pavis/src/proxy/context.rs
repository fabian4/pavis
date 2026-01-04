use pavis_core::{HeadersPolicy, Hostname, UpstreamName};

pub struct RouterContext {
    pub upstream_name: Option<UpstreamName>,
    pub request_headers: HeadersPolicy,
    pub response_headers: HeadersPolicy,
    pub sni_override: Option<Hostname>,
    pub start_time: std::time::Instant,
}

#[cfg(test)]
mod tests {
    use super::RouterContext;
    use pavis_core::{HeaderName, HeaderValue, Headers, HeadersPolicy, UpstreamName};

    #[test]
    fn router_context_holds_fields() {
        let ctx = RouterContext {
            upstream_name: Some(UpstreamName("backend".to_string())),
            request_headers: HeadersPolicy::Enabled {
                rules: Headers {
                    set_headers: vec![(
                        HeaderName("x-test".to_string()),
                        HeaderValue("1".to_string()),
                    )],
                    append_headers: Vec::new(),
                    add_headers: Vec::new(),
                    remove_headers: Vec::new(),
                },
            },
            response_headers: HeadersPolicy::Disabled,
            sni_override: None,
            start_time: std::time::Instant::now(),
        };

        assert_eq!(
            ctx.upstream_name.as_ref().map(|v| v.0.as_str()),
            Some("backend")
        );
    }
}
