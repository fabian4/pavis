pub mod delay;
pub mod echo;
pub mod failure;
pub mod healthz;
pub mod reset;
pub mod status;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router, async_trait,
    extract::{ConnectInfo, FromRequestParts, Query, State},
    http::{self, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::upstream::types::{IdResponse, StatusResponse, StubResponse};

const TEST_RUN_HEADER: &str = "x-pavis-test-run";
const TEST_CASE_HEADER: &str = "x-pavis-test-case";

#[derive(Clone, Default)]
struct FlakyState {
    counters: HashMap<String, u32>,
}

#[derive(Clone)]
pub struct SharedState {
    instance_id: Arc<String>,
    global_delay_ms: Option<u64>,
    flaky: Arc<Mutex<FlakyState>>,
    pub(crate) failure_config: Arc<Mutex<Vec<crate::common::cli::FailureRule>>>,
    pub(crate) failure_counter: Arc<AtomicU32>,
}

impl SharedState {
    pub fn new(instance_id: String) -> Self {
        Self::with_config(instance_id, None, None)
    }

    pub fn with_delay(instance_id: String, delay_ms: Option<u64>) -> Self {
        Self::with_config(instance_id, delay_ms, None)
    }

    pub fn with_config(
        instance_id: String,
        delay_ms: Option<u64>,
        failure_sequence: Option<Vec<crate::common::cli::FailureRule>>,
    ) -> Self {
        Self {
            instance_id: Arc::new(instance_id),
            global_delay_ms: delay_ms,
            flaky: Arc::new(Mutex::new(FlakyState::default())),
            failure_config: Arc::new(Mutex::new(failure_sequence.unwrap_or_default())),
            failure_counter: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn global_delay_ms(&self) -> Option<u64> {
        self.global_delay_ms
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

#[derive(serde::Deserialize)]
pub struct FlakyQuery {
    #[serde(default = "default_flaky_code")]
    code: u16,
    #[serde(default = "default_flaky_times")]
    times: u32,
    #[serde(default)]
    delay_ms: u64,
    id: String,
}

fn default_flaky_code() -> u16 {
    503
}
fn default_flaky_times() -> u32 {
    1
}

#[derive(serde::Deserialize)]
pub struct ResetQuery {
    id: Option<String>,
}

async fn flaky_handler(
    State(state): State<ServerState>,
    Query(params): Query<FlakyQuery>,
    ctx: TestContext,
) -> Response {
    if params.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(params.delay_ms)).await;
    }

    let mut flaky = state.shared.flaky.lock().expect("lock poisoned");
    let count = flaky.counters.entry(params.id.clone()).or_insert(0);

    tracing::info!(id = %params.id, count_before = *count, times = params.times, "flaky check start");

    if *count < params.times {
        *count += 1;
        tracing::info!(id = %params.id, count_after = *count, "flaky failing");
        let status = StatusCode::from_u16(params.code).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
        let mut resp = ctx.respond(
            status,
            StatusResponse {
                status: status.as_u16(),
                ok: false,
            },
        );
        resp.headers_mut()
            .insert(http::header::CONNECTION, HeaderValue::from_static("close"));
        resp
    } else {
        tracing::info!(id = %params.id, count = *count, "flaky success");
        ctx.respond(
            StatusCode::OK,
            StatusResponse {
                status: 200,
                ok: true,
            },
        )
    }
}

async fn reset_handler(
    State(state): State<ServerState>,
    Query(params): Query<ResetQuery>,
    ctx: TestContext,
) -> Response {
    let mut flaky = state.shared.flaky.lock().expect("lock poisoned");
    if let Some(id) = params.id {
        flaky.counters.remove(&id);
    } else {
        flaky.counters.clear();
    }

    ctx.respond(
        StatusCode::OK,
        IdResponse {
            id: "reset_ok".to_string(),
        },
    )
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
        .route("/flaky", any(flaky_handler))
        .route("/failure", any(failure::handler))
        .route("/received", get(stub_received))
        .route("/reset", any(reset_handler))
        .fallback(any(echo::handler))
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

async fn stub_bytes(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/bytes"))
}

async fn stub_hang(ctx: TestContext) -> Response {
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    ctx.respond(
        StatusCode::OK,
        IdResponse {
            id: "hang_ok".to_string(),
        },
    )
}

async fn stub_close(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/close"))
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

async fn instance_id(State(state): State<ServerState>, ctx: TestContext) -> Response {
    ctx.respond(
        StatusCode::OK,
        IdResponse {
            id: state.instance_id().to_string(),
        },
    )
}
