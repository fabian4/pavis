use std::sync::Arc;
use std::time::Instant;

use pavis::proxy::context::{
    RequestId, RequestTelemetry, RouterContext, RoutePattern, UpstreamTiming,
};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis_core::{HeadersPolicy, Hostname, RetryPolicy, Timeout, UpstreamName};

/// Builder that produces lightweight RouterContext instances for request-planning tests.
pub struct RouterContextBuilder {
    ctx: RouterContext,
}

impl RouterContextBuilder {
    pub fn new() -> Self {
        let telemetry = RequestTelemetry::new(RequestId::from_parts(0, 1));
        let ctx = RouterContext {
            telemetry,
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
        Self { ctx }
    }

    pub fn with_upstream(mut self, name: &str) -> Self {
        self.ctx.upstream_name = Some(UpstreamName(name.to_string()));
        self
    }

    pub fn with_sni_override(mut self, hostname: &str) -> Self {
        self.ctx.sni_override = Some(Hostname(hostname.to_string()));
        self
    }

    pub fn with_runtime_state(mut self, state: RuntimeState) -> Self {
        self.ctx.runtime_state = Some(Arc::new(state));
        self
    }

    pub fn build(self) -> RouterContext {
        self.ctx
    }
}

/// Convenience helper for the common case where defaults are sufficient.
pub fn mock_router_context() -> RouterContext {
    RouterContextBuilder::new().build()
}

/// Creates an isolated runtime state handle suitable for tests.
pub fn mock_runtime_state_handle() -> RuntimeStateHandle {
    RuntimeStateHandle::new(RuntimeState::default())
}

#[test]
fn builder_sets_upstream_name() {
    let ctx = RouterContextBuilder::new()
        .with_upstream("backend")
        .with_sni_override("example.com")
        .build();

    assert_eq!(ctx.upstream_label(), "backend");
    assert_eq!(ctx.sni_override.as_ref().map(|h| h.0.as_str()), Some("example.com"));
}

#[test]
fn builder_accepts_runtime_state() {
    let state = RuntimeState::default();
    let ctx = RouterContextBuilder::new().with_runtime_state(state).build();
    assert!(ctx.runtime_state.is_some());
}

#[test]
fn runtime_state_helper_provides_snapshot() {
    let handle = mock_runtime_state_handle();
    let snapshot = handle.load();
    assert!(snapshot.config_version.is_none());
}
