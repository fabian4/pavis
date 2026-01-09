use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Json, Router, async_trait,
    body::{self, Body},
    extract::{ConnectInfo, FromRequestParts, Query, State},
    http::{self, HeaderMap, HeaderValue, Request, StatusCode, Version, header::HeaderName},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::Deserialize;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::types::{
    DelayResponse, EchoResponse, HealthResponse, IdResponse, StatusResponse, StubResponse,
    TlsDetails,
};

const MAX_DELAY_MS: u64 = 60_000;
const MAX_ECHO_BODY: usize = 1024 * 1024;
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
        .route("/healthz", get(health))
        .route("/echo", any(echo))
        .route("/id", get(instance_id))
        .route("/status", get(status))
        .route("/delay", get(delay))
        .route("/bytes", get(stub_bytes))
        .route("/hang", get(stub_hang))
        .route("/close", get(stub_close))
        .route("/flaky", get(stub_flaky))
        .route("/received", get(stub_received))
        .route("/reset", post(stub_reset))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state)
}

#[derive(Debug, Clone)]
pub struct TestContext {
    run: Option<String>,
    case: Option<String>,
}

impl TestContext {
    fn respond<T>(&self, status: StatusCode, payload: T) -> Response
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

#[derive(Deserialize)]
struct StatusQuery {
    code: Option<u16>,
}

#[derive(Deserialize)]
struct DelayQuery {
    ms: Option<u64>,
}

async fn health(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::OK, HealthResponse { ok: true })
}

async fn instance_id(State(state): State<ServerState>, ctx: TestContext) -> Response {
    ctx.respond(
        StatusCode::OK,
        IdResponse {
            id: state.instance_id().to_string(),
        },
    )
}

async fn status(Query(params): Query<StatusQuery>, ctx: TestContext) -> Response {
    let code = params.code.unwrap_or(StatusCode::OK.as_u16());
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    let ok = status.is_success();

    ctx.respond(
        status,
        StatusResponse {
            status: status.as_u16(),
            ok,
        },
    )
}

async fn delay(Query(params): Query<DelayQuery>, ctx: TestContext) -> Response {
    let requested = params.ms.unwrap_or(0);
    let bounded = requested.min(MAX_DELAY_MS);
    tokio::time::sleep(Duration::from_millis(bounded)).await;

    ctx.respond(
        StatusCode::OK,
        DelayResponse {
            delayed_ms: bounded,
        },
    )
}

async fn echo(
    State(state): State<ServerState>,
    RemoteAddress(remote_addr): RemoteAddress,
    ctx: TestContext,
    request: Request<Body>,
) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let version = request.version();
    let headers = request.headers().clone();

    let body_bytes = match body::to_bytes(request.into_body(), MAX_ECHO_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return ctx.respond(
                StatusCode::PAYLOAD_TOO_LARGE,
                StubResponse {
                    error: "body_too_large",
                    endpoint: "/echo",
                    note: "payload exceeded limit",
                },
            );
        }
    };

    let response = EchoResponse {
        instance_id: state.instance_id().to_string(),
        method: method.to_string(),
        path: uri.path().to_string(),
        query: uri.query().unwrap_or_default().to_string(),
        protocol: version_string(version),
        tls: TlsDetails {
            enabled: state.transport().tls_enabled(),
            version: None,
            sni: None,
        },
        headers: canonical_headers(&headers),
        body_len: body_bytes.len(),
        remote_addr: remote_addr.map(|addr| addr.to_string()),
    };

    ctx.respond(StatusCode::OK, response)
}

async fn stub_bytes(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/bytes"))
}

async fn stub_hang(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/hang"))
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

async fn stub_reset(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::NOT_IMPLEMENTED, stub_response("/reset"))
}

fn stub_response(endpoint: &'static str) -> StubResponse {
    StubResponse {
        error: "not_implemented",
        endpoint,
        note: "TODO",
    }
}

fn canonical_headers(headers: &HeaderMap) -> BTreeMap<String, Vec<String>> {
    let mut canonical = BTreeMap::new();

    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        let entry = canonical.entry(key).or_insert_with(Vec::new);
        match value.to_str() {
            Ok(text) => entry.push(text.to_string()),
            Err(_) => entry.push(String::new()),
        }
    }

    canonical
}

fn version_string(version: Version) -> Option<String> {
    match version {
        Version::HTTP_09 => Some("HTTP/0.9".to_string()),
        Version::HTTP_10 => Some("HTTP/1.0".to_string()),
        Version::HTTP_11 => Some("HTTP/1.1".to_string()),
        Version::HTTP_2 => Some("HTTP/2".to_string()),
        Version::HTTP_3 => Some("HTTP/3".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_echoes_test_headers() {
        let shared = SharedState::new("test-instance".to_string());
        let app = router(shared, TransportMeta::http());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .header("X-Pavis-Test-Run", "run-1")
                    .header("X-Pavis-Test-Case", "case-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("x-pavis-test-run")
                .map(|value: &axum::http::HeaderValue| value.to_str().unwrap()),
            Some("run-1")
        );
        assert_eq!(
            response
                .headers()
                .get("x-pavis-test-case")
                .map(|value: &axum::http::HeaderValue| value.to_str().unwrap()),
            Some("case-a"),
        );
    }
}
