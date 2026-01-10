pub mod delay;
pub mod echo;
pub mod healthz;
pub mod reset;
pub mod status;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{self, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::upstream::types::{IdResponse, StubResponse};

const TEST_RUN_HEADER: &str = "x-pavis-test-run";
const TEST_CASE_HEADER: &str = "x-pavis-test-case";

#[derive(Clone)]
pub struct SharedState {
    instance_id: Arc<String>,
}

impl SharedState {
    pub fn new(instance_id: String) -> Self {
        Self {
            instance_id: Arc::new(instance_id),
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[derive(Clone)]
pub struct ServerState {
    shared: SharedState,
    transport: TransportMeta,
}

impl ServerState {
    pub fn new(shared: SharedState, transport: TransportMeta) -> Self {
        Self { shared, transport }
    }

    pub fn instance_id(&self) -> &str {
        self.shared.instance_id()
    }

    pub fn transport(&self) -> TransportMeta {
        self.transport
    }
}

#[derive(Clone, Copy)]
pub struct TransportMeta {
    tls_enabled: bool,
}

impl TransportMeta {
    pub const fn http() -> Self {
        Self { tls_enabled: false }
    }

    pub const fn https() -> Self {
        Self { tls_enabled: true }
    }

    pub const fn tls_enabled(&self) -> bool {
        self.tls_enabled
    }
}

pub fn router(shared: SharedState, transport: TransportMeta) -> Router {
    let state = ServerState::new(shared, transport);

    Router::new()
        .route("/healthz", get(healthz::handler))
        .route("/echo", any(echo::handler))
        .route("/id", get(instance_id))
        .route("/status", get(status::handler))
        .route("/delay", get(delay::handler))
        .route("/bytes", get(stub_bytes))
        .route("/hang", get(stub_hang))
        .route("/close", get(stub_close))
        .route("/flaky", get(stub_flaky))
        .route("/received", get(stub_received))
        .route("/reset", post(reset::handler))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state)
}

#[derive(Debug, Clone)]
pub struct TestContext {
    run: Option<String>,
    case: Option<String>,
}

impl TestContext {
    pub fn respond<T>(&self, status: StatusCode, payload: T) -> Response
    where
        T: serde::Serialize,
    {
        let mut response = (status, Json(payload)).into_response();
        self.apply_headers(&mut response);
        response
    }

    fn apply_headers(&self, response: &mut Response) {
        if let Some(Ok(value)) = self.run.as_deref().map(HeaderValue::from_str) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(TEST_RUN_HEADER), value);
        }

        if let Some(Ok(value)) = self.case.as_deref().map(HeaderValue::from_str) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(TEST_CASE_HEADER), value);
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for TestContext
where
    S: Send + Sync,
{
    type Rejection = ();

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let run = parts
            .headers
            .get(TEST_RUN_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let case = parts
            .headers
            .get(TEST_CASE_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        Ok(Self { run, case })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RemoteAddress(pub Option<SocketAddr>);

#[async_trait]
impl<S> FromRequestParts<S> for RemoteAddress
where
    S: Send + Sync,
{
    type Rejection = ();

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let addr = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| *addr);
        Ok(Self(addr))
    }
}

async fn instance_id(State(state): State<ServerState>, ctx: TestContext) -> Response {
    ctx.respond(
        StatusCode::OK,
        IdResponse {
            id: state.instance_id().to_string(),
        },
    )
}

async fn stub_bytes(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/bytes"))
}

async fn stub_hang(ctx: TestContext) -> Response {
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    ctx.respond(StatusCode::OK, stub_response("/hang"))
}

async fn stub_close(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/close"))
}

async fn stub_flaky(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/flaky"))
}

async fn stub_received(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/received"))
}

fn stub_response(endpoint: &'static str) -> StubResponse {
    StubResponse {
        error: "not_implemented",
        endpoint,
        note: "TODO",
    }
}
