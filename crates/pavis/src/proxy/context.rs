use crate::retry::{BufferedBody, RetryContext};
use crate::state::RuntimeState;
use http::Uri;
use pavis_core::{
    EndpointAddr, HeadersPolicy, Hostname, RetryPolicy, SpiffeId, Timeout, UpstreamName,
};
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

#[derive(Debug)]
pub enum TracingSpan {
    Disabled,
    Active(tracing::Span),
}

#[derive(Debug)]
pub struct RequestTelemetry {
    request_id: RequestId,
    span: TracingSpan,
}

impl RequestTelemetry {
    pub fn new(request_id: RequestId) -> Self {
        Self {
            request_id,
            span: TracingSpan::Disabled,
        }
    }

    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub fn set_request_id(&mut self, request_id: RequestId) {
        self.request_id = request_id;
    }

    pub fn span(&self) -> &TracingSpan {
        &self.span
    }

    pub fn span_mut(&mut self) -> &mut TracingSpan {
        &mut self.span
    }

    pub fn replace_span(&mut self, span: TracingSpan) {
        self.span = span;
    }
}

pub struct RouterContext {
    pub telemetry: RequestTelemetry,
    pub upstream_name: Option<UpstreamName>,
    pub upstream_endpoint: Option<EndpointAddr>,
    pub request_headers: Arc<HeadersPolicy>,
    pub response_headers: Arc<HeadersPolicy>,
    pub sni_override: Option<Hostname>,
    pub start_time: Instant,
    pub upstream_timing: UpstreamTiming,
    pub client_identity: Option<SpiffeId>,
    pub rbac_denied: bool,
    pub route_timeout: Timeout,
    pub retry_policy: RetryPolicy,
    pub retry_attempts: u16,
    pub route_pattern: RoutePattern,
    pub pool_permit: Option<crate::upstream::cluster::PoolPermit>,
    pub circuit_breaker_permit: Option<OwnedSemaphorePermit>,
    /// Pinned configuration snapshot for this request.
    /// Captured in `request_filter` to ensure atomicity across routing and upstream selection.
    pub runtime_state: Option<Arc<RuntimeState>>,

    /// P2 Retry context
    pub retry_ctx: Option<RetryContext>,
    /// P2 Buffered request body
    pub buffered_body: Option<BufferedBody>,
    /// Optional URI after path rewrite
    pub rewritten_uri: Option<Uri>,
    /// Optional Host after host rewrite
    pub rewritten_host: Option<Hostname>,
}

// Phase-typed wrappers enforce request lifecycle transitions.
pub struct RoutingContext<'ctx> {
    ctx: &'ctx mut RouterContext,
}

pub struct RouteMatch<'ctx> {
    ctx: &'ctx mut RouterContext,
}

pub struct UpstreamAttempt<'ctx> {
    ctx: &'ctx mut RouterContext,
}

impl<'ctx> RoutingContext<'ctx> {
    pub fn attach_runtime(&mut self, state: Arc<RuntimeState>) {
        self.ctx.runtime_state = Some(state);
    }

    pub fn enable_tracing(&mut self, span: tracing::Span) {
        self.ctx.telemetry.replace_span(TracingSpan::Active(span));
    }

    pub fn record_route(self, pattern: Arc<str>) -> RouteMatch<'ctx> {
        self.ctx.route_pattern = RoutePattern::Matched { pattern };
        RouteMatch { ctx: self.ctx }
    }
}

impl<'ctx> RouteMatch<'ctx> {
    pub fn ctx(&self) -> &RouterContext {
        self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut RouterContext {
        self.ctx
    }

    pub fn client_identity(&self) -> Option<&SpiffeId> {
        self.ctx.client_identity.as_ref()
    }

    pub fn request_id(&self) -> RequestId {
        self.ctx.request_id()
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.ctx.retry_policy
    }

    pub fn route_timeout(&self) -> Timeout {
        self.ctx.route_timeout
    }

    pub fn record_route_span(&self) {
        if let RoutePattern::Matched { ref pattern } = self.ctx.route_pattern
            && let TracingSpan::Active(span) = self.ctx.span()
        {
            span.record("route.pattern", pattern.as_ref());
        }
    }

    pub fn mark_rbac_denied(&mut self) {
        self.ctx.rbac_denied = true;
    }

    pub fn set_rewritten_uri(&mut self, uri: Uri) {
        self.ctx.rewritten_uri = Some(uri);
    }

    pub fn set_rewritten_host(&mut self, host: Hostname) {
        self.ctx.rewritten_host = Some(host);
    }

    pub fn set_retry_context(&mut self, retry: RetryContext) {
        self.ctx.retry_ctx = Some(retry);
    }

    pub fn set_buffered_body(&mut self, body: BufferedBody) {
        self.ctx.buffered_body = Some(body);
    }

    pub fn into_upstream_attempt(self) -> UpstreamAttempt<'ctx> {
        UpstreamAttempt { ctx: self.ctx }
    }
}

impl<'ctx> UpstreamAttempt<'ctx> {
    pub fn set_upstream(&mut self, upstream: UpstreamName) {
        self.ctx.upstream_name = Some(upstream);
    }

    pub fn record_upstream_span(&self, name: &str) {
        if let TracingSpan::Active(span) = self.ctx.span() {
            span.record("upstream", name);
        }
    }

    pub fn set_endpoint(&mut self, endpoint: EndpointAddr) {
        self.ctx.upstream_endpoint = Some(endpoint);
    }

    pub fn store_pool_permit(&mut self, permit: crate::upstream::cluster::PoolPermit) {
        self.ctx.pool_permit = Some(permit);
    }

    pub fn store_breaker_permit(&mut self, permit: OwnedSemaphorePermit) {
        self.ctx.circuit_breaker_permit = Some(permit);
    }

    pub fn start_upstream(&mut self) {
        self.ctx.start_upstream();
    }
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

    pub fn as_label_opt(&self) -> Option<&str> {
        match self {
            RoutePattern::NotMatched => None,
            RoutePattern::Matched { pattern } => Some(pattern),
        }
    }
}

impl RouterContext {
    pub fn routing_phase(&mut self) -> RoutingContext<'_> {
        RoutingContext { ctx: self }
    }

    pub fn request_duration(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn upstream_latency(&self) -> Option<std::time::Duration> {
        self.upstream_timing.elapsed()
    }

    pub fn request_id(&self) -> RequestId {
        self.telemetry.request_id()
    }

    pub fn set_request_id(&mut self, request_id: RequestId) {
        self.telemetry.set_request_id(request_id);
    }

    pub fn span(&self) -> &TracingSpan {
        self.telemetry.span()
    }

    pub fn span_mut(&mut self) -> &mut TracingSpan {
        self.telemetry.span_mut()
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
            telemetry: RequestTelemetry::new("req-123".parse().unwrap()),
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
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
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
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        assert_eq!(ctx.upstream_label(), "-");
    }

    #[test]
    fn start_upstream_updates_timing() {
        let mut ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: Some(UpstreamName("backend".to_string())),
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::Matched {
                pattern: Arc::from("/api"),
            },
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
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
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        std::thread::sleep(std::time::Duration::from_millis(10));
        let duration = ctx.request_duration();
        assert!(duration >= std::time::Duration::from_millis(10));
    }

    #[test]
    fn routing_phase_attaches_runtime_state_and_sets_pattern() {
        let state = Arc::new(RuntimeState::default());
        let mut ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-ctx".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        {
            let mut phase = ctx.routing_phase();
            phase.attach_runtime(state.clone());
            let _match = phase.record_route(Arc::from("/plan"));
        }

        assert!(ctx.runtime_state.is_some());
        assert_eq!(ctx.route_pattern.as_label(), "/plan");
    }

    #[test]
    fn route_match_transitions_to_upstream_attempt() {
        let mut ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-upstream".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        {
            let route = ctx.routing_phase().record_route(Arc::from("/ready"));
            let mut attempt = route.into_upstream_attempt();
            attempt.set_upstream(UpstreamName("backend".to_string()));
            attempt.set_endpoint(EndpointAddr::Dns {
                host: pavis_core::Hostname("example.com".to_string()),
                port: pavis_core::Port(std::num::NonZeroU16::new(443).unwrap()),
            });
            attempt.start_upstream();
        }

        assert_eq!(ctx.upstream_label(), "backend");
        assert!(matches!(ctx.upstream_timing, UpstreamTiming::Started(_)));
    }

    #[test]
    fn request_id_is_utf8() {
        let id = RequestId::from_parts(0, 42);
        assert!(std::str::from_utf8(id.as_str().as_bytes()).is_ok());
        assert!(id.as_str().starts_with("req-"));
    }
}
