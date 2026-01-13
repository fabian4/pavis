use pavis_core::{HeadersPolicy, Hostname, UpstreamName};
use std::sync::Arc;
use std::time::Instant;

pub struct RouterContext {
    pub upstream_name: Option<UpstreamName>,
    pub request_headers: Arc<HeadersPolicy>,
    pub response_headers: Arc<HeadersPolicy>,
    pub sni_override: Option<Hostname>,
    pub start_time: Instant,
    pub upstream_timing: UpstreamTiming,
    pub client_identity: Option<String>,
    pub rbac_denied: bool,
    pub route_pattern: RoutePattern,
    pub req_id: String,
    pub span: TracingSpan,
}

#[derive(Debug, Clone)]
pub enum UpstreamTiming {
    NotStarted,
    Started(Instant),
}

impl UpstreamTiming {
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        match self {
            UpstreamTiming::NotStarted => None,
            UpstreamTiming::Started(start) => Some(start.elapsed()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RoutePattern {
    NotMatched,
    Matched { pattern: Arc<str> },
}

impl RoutePattern {
    pub fn as_label(&self) -> &str {
        match self {
            RoutePattern::NotMatched => "-",
            RoutePattern::Matched { pattern } => pattern,
        }
    }
}

#[derive(Debug)]
pub enum TracingSpan {
    Disabled,
    Active(tracing::Span),
}

impl RouterContext {
    pub fn request_duration(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn upstream_latency(&self) -> Option<std::time::Duration> {
        self.upstream_timing.elapsed()
    }

    pub fn start_upstream(&mut self) {
        self.upstream_timing = UpstreamTiming::Started(Instant::now());
    }

    pub fn upstream_label(&self) -> &str {
        self.upstream_name
            .as_ref()
            .map(|n| n.0.as_str())
            .unwrap_or("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{HeaderName, HeaderValue, Headers, HeadersPolicy, UpstreamName};

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
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            req_id: "req-123".to_string(),
            span: TracingSpan::Disabled,
        };

        assert_eq!(
            ctx.upstream_name.as_ref().map(|v| v.0.as_str()),
            Some("backend")
        );
    }

    #[test]
    fn route_pattern_not_matched_returns_dash() {
        let pattern = RoutePattern::NotMatched;
        assert_eq!(pattern.as_label(), "-");
    }

    #[test]
    fn route_pattern_matched_returns_pattern() {
        let pattern = RoutePattern::Matched {
            pattern: Arc::from("/users/:id"),
        };
        assert_eq!(pattern.as_label(), "/users/:id");
    }

    #[test]
    fn upstream_label_returns_dash_when_not_selected() {
        let ctx = RouterContext {
            upstream_name: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            req_id: "req-1".to_string(),
            span: TracingSpan::Disabled,
        };

        assert_eq!(ctx.upstream_label(), "-");
    }

    #[test]
    fn start_upstream_updates_timing() {
        let mut ctx = RouterContext {
            upstream_name: Some(UpstreamName("backend".to_string())),
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::Matched {
                pattern: Arc::from("/api"),
            },
            req_id: "req-1".to_string(),
            span: TracingSpan::Disabled,
        };

        ctx.start_upstream();
        assert!(matches!(ctx.upstream_timing, UpstreamTiming::Started(_)));
    }

    #[test]
    fn upstream_timing_elapsed_returns_none_when_not_started() {
        let timing = UpstreamTiming::NotStarted;
        assert!(timing.elapsed().is_none());
    }

    #[test]
    fn request_duration_calculates_elapsed_time() {
        let ctx = RouterContext {
            upstream_name: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            req_id: "req-1".to_string(),
            span: TracingSpan::Disabled,
        };

        std::thread::sleep(std::time::Duration::from_millis(10));
        let duration = ctx.request_duration();
        assert!(duration >= std::time::Duration::from_millis(10));
    }
}
