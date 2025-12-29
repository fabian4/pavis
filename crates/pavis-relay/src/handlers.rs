use crate::state::RelayState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub(crate) struct ConfigQuery {
    pub(crate) wait_ms: Option<u64>,
}

pub(crate) async fn get_config(
    State(state): State<Arc<RelayState>>,
    headers: HeaderMap,
    Query(query): Query<ConfigQuery>,
) -> Response {
    let current_version = state.version().await;
    let client_version = headers
        .get("x-pavis-version")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let wait_ms = query.wait_ms.unwrap_or(1000).min(10_000);

    if client_version == Some(current_version) {
        let notified = state.notifier().notified();
        let _ = tokio::time::timeout(std::time::Duration::from_millis(wait_ms), notified).await;
        let latest_version = state.version().await;
        if latest_version == current_version {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    let (version, pvs_bytes) = state.snapshot().await;
    let mut response = pvs_bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "x-pavis-version",
        HeaderValue::from_str(&version.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response
}

pub(crate) async fn get_status(State(state): State<Arc<RelayState>>) -> Response {
    let version = state.version().await;
    let body = format!("version={version}\n");
    (StatusCode::OK, body).into_response()
}

pub(crate) async fn get_health() -> Response {
    (StatusCode::OK, "ok\n").into_response()
}

pub(crate) async fn get_ready() -> Response {
    (StatusCode::OK, "ready\n").into_response()
}

pub(crate) async fn post_publish(
    State(state): State<Arc<RelayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let proposed_version = match headers
        .get("x-pavis-version")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(version) => version,
        None => return (StatusCode::BAD_REQUEST, "missing x-pavis-version\n").into_response(),
    };

    if let Err(err) = state.publish(proposed_version, body).await {
        return (StatusCode::CONFLICT, format!("{err}\n")).into_response();
    }

    (StatusCode::OK, "ok\n").into_response()
}

pub(crate) async fn get_artifact(
    State(state): State<Arc<RelayState>>,
    Path(version): Path<u64>,
) -> Response {
    match state.artifact(version).await {
        Some(bytes) => bytes.into_response(),
        None => (StatusCode::NOT_FOUND, "unknown version\n").into_response(),
    }
}

pub(crate) async fn get_metrics(State(state): State<Arc<RelayState>>) -> Response {
    let version = state.version().await;
    let body = format!(
        "# HELP pavis_relay_version Current config version\n# TYPE pavis_relay_version gauge\npavis_relay_version {version}\n"
    );
    (StatusCode::OK, body).into_response()
}
