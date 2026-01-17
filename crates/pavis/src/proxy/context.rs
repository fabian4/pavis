use crate::state::RuntimeState;
use pavis_core::{EndpointAddr, HeadersPolicy, Hostname, UpstreamName};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::OwnedSemaphorePermit;

const REQUEST_ID_MAX_LEN: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RequestId {
    buf: [u8; REQUEST_ID_MAX_LEN],
    len: u8,
}

#[derive(Debug)]
pub struct RequestIdParseError;

impl std::fmt::Display for RequestIdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("request id is too long")
    }
}

impl std::error::Error for RequestIdParseError {}

impl RequestId {
    pub fn from_parts(nanos: u128, random: u32) -> Self {
        let mut id = Self::empty();
        id.push_bytes(b"req-");
        id.push_u128(nanos);
        id.push_byte(b'-');
        id.push_u128(u128::from(random));
        id
    }

    pub fn as_str(&self) -> &str {
        let len = self.len as usize;
        // SAFETY: RequestId only stores ASCII bytes (digits and separators). Length is bounded.
        unsafe { std::str::from_utf8_unchecked(&self.buf[..len]) }
    }

    fn empty() -> Self {
        Self {
            buf: [0; REQUEST_ID_MAX_LEN],
            len: 0,
        }
    }

    fn push_byte(&mut self, value: u8) {
        let len = self.len as usize;
        debug_assert!(len < REQUEST_ID_MAX_LEN);
        if len >= REQUEST_ID_MAX_LEN {
            return;
        }
        self.buf[len] = value;
        self.len = (len + 1) as u8;
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        let len = self.len as usize;
        debug_assert!(len + bytes.len() <= REQUEST_ID_MAX_LEN);
        if len + bytes.len() > REQUEST_ID_MAX_LEN {
            return;
        }
        self.buf[len..len + bytes.len()].copy_from_slice(bytes);
        self.len = (len + bytes.len()) as u8;
    }

    fn push_u128(&mut self, mut value: u128) {
        if value == 0 {
            self.push_byte(b'0');
            return;
        }

        let mut tmp = [0u8; 39];
        let mut idx = 0;
        while value > 0 {
            let digit = (value % 10) as u8;
            tmp[idx] = b'0' + digit;
            idx += 1;
            value /= 10;
        }

        for pos in (0..idx).rev() {
            self.push_byte(tmp[pos]);
        }
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RequestId {
    type Err = RequestIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > REQUEST_ID_MAX_LEN {
            return Err(RequestIdParseError);
        }
        let mut id = Self::empty();
        id.push_bytes(value.as_bytes());
        Ok(id)
    }
}

impl Serialize for RequestId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

pub struct RouterContext {
    pub upstream_name: Option<UpstreamName>,
    pub upstream_endpoint: Option<EndpointAddr>,
    pub request_headers: Arc<HeadersPolicy>,
    pub response_headers: Arc<HeadersPolicy>,
    pub sni_override: Option<Hostname>,
    pub start_time: Instant,
    pub upstream_timing: UpstreamTiming,
    pub client_identity: Option<String>,
    pub rbac_denied: bool,
    pub route_pattern: RoutePattern,
    pub req_id: RequestId,
    pub span: TracingSpan,
    pub circuit_breaker_permit: Option<OwnedSemaphorePermit>,
    /// Pinned configuration snapshot for this request.
    /// Captured in `request_filter` to ensure atomicity across routing and upstream selection.
    pub runtime_state: Option<Arc<RuntimeState>>,
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
            upstream_endpoint: None,
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
            req_id: "req-123".parse().unwrap(),
            span: TracingSpan::Disabled,
            circuit_breaker_permit: None,
            runtime_state: None,
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
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            req_id: "req-1".parse().unwrap(),
            span: TracingSpan::Disabled,
            circuit_breaker_permit: None,
            runtime_state: None,
        };

        assert_eq!(ctx.upstream_label(), "-");
    }

    #[test]
    fn start_upstream_updates_timing() {
        let mut ctx = RouterContext {
            upstream_name: Some(UpstreamName("backend".to_string())),
            upstream_endpoint: None,
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
            req_id: "req-1".parse().unwrap(),
            span: TracingSpan::Disabled,
            circuit_breaker_permit: None,
            runtime_state: None,
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
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            req_id: "req-1".parse().unwrap(),
            span: TracingSpan::Disabled,
            circuit_breaker_permit: None,
            runtime_state: None,
        };

        std::thread::sleep(std::time::Duration::from_millis(10));
        let duration = ctx.request_duration();
        assert!(duration >= std::time::Duration::from_millis(10));
    }

    #[test]
    fn request_id_is_utf8() {
        let id = RequestId::from_parts(0, 42);
        assert!(std::str::from_utf8(id.as_str().as_bytes()).is_ok());
        assert!(id.as_str().starts_with("req-"));
    }
}
