use crate::state::RelayState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use pavis_pvs::{inspect, verify};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(serde::Deserialize)]
pub(crate) struct ConfigQuery {
    pub(crate) wait_ms: Option<u64>,
}

pub(crate) async fn get_config(
    State(state): State<Arc<RelayState>>,
    headers: HeaderMap,
    Query(query): Query<ConfigQuery>,
) -> Response {
    let options = state.options().clone();
    let client_version = match headers
        .get(&options.version_header)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(version) => version,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                format!("missing {}\n", options.version_header.as_str()),
            )
                .into_response();
        }
    };
    let current_version = state.version().await;
    let wait_ms = query.wait_ms.unwrap_or(1000).min(10_000);

    if client_version == current_version && options.long_poll_enabled {
        let notified = state.notifier().notified();
        let _ = tokio::time::timeout(std::time::Duration::from_millis(wait_ms), notified).await;
        let latest_version = state.version().await;
        if latest_version == current_version {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    let snapshot = state.snapshot().await;
    let meta = match inspect(&snapshot.pvs_bytes) {
        Ok(meta) => meta,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response(),
    };
    let checksum = meta.checksum_hex();
    let algorithm = meta.algorithm_label();

    let mut response = snapshot.pvs_bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        options.version_header,
        HeaderValue::from_str(&snapshot.version.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    headers.insert(
        options.checksum_header,
        HeaderValue::from_str(&checksum).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        options.checksum_alg_header,
        HeaderValue::from_str(&algorithm).unwrap_or_else(|_| HeaderValue::from_static("sha256")),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn get_status(State(state): State<Arc<RelayState>>) -> Response {
    let options = state.options();
    let snapshot = state.snapshot().await;
    let version = snapshot.version;
    let size = snapshot.pvs_bytes.len();
    let (checksum, algorithm) = match inspect(&snapshot.pvs_bytes) {
        Ok(meta) => (meta.checksum_hex(), meta.algorithm_label()),
        Err(_) => ("invalid".to_string(), "unknown".to_string()),
    };
    let updated_at = format_unix_time(snapshot.updated_at);
    let body = format!(
        "name={} version={version} checksum={checksum} checksum_alg={algorithm} size={size} updated_at={updated_at}\n",
        options.identity_name
    );
    (StatusCode::OK, body).into_response()
}

pub(crate) async fn get_health() -> Response {
    (StatusCode::OK, "ok\n").into_response()
}

pub(crate) async fn get_ready(State(state): State<Arc<RelayState>>) -> Response {
    let snapshot = state.snapshot().await;
    if snapshot.pvs_bytes.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "no artifact\n").into_response();
    }
    (StatusCode::OK, "ready\n").into_response()
}

pub(crate) async fn post_publish(
    State(state): State<Arc<RelayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let options = state.options().clone();
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty body\n").into_response();
    }
    let proposed_version = match headers
        .get(&options.version_header)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(version) => version,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                format!("missing {}\n", options.version_header.as_str()),
            )
                .into_response();
        }
    };

    if let Err(err) = verify(&body) {
        return (StatusCode::UNPROCESSABLE_ENTITY, format!("{err}\n")).into_response();
    }

    let payload = body.clone();
    if let Err(err) = state.publish(proposed_version, body).await {
        return (StatusCode::CONFLICT, format!("{err}\n")).into_response();
    }

    if let Some(path) = options.lkg_path.as_ref() {
        if let Some(parent) = path.parent() {
            match tokio::fs::create_dir_all(parent).await {
                Ok(()) => {}
                Err(err) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response();
                }
            }
        }
        if let Err(err) = tokio::fs::write(path, &payload).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response();
        }
    }

    (StatusCode::OK, "ok\n").into_response()
}

pub(crate) async fn get_artifact(
    State(state): State<Arc<RelayState>>,
    Path(version): Path<u64>,
) -> Response {
    let options = state.options().clone();
    let bytes = match state.artifact(version).await {
        Some(bytes) => bytes,
        None => return (StatusCode::NOT_FOUND, "unknown version\n").into_response(),
    };

    let meta = match inspect(&bytes) {
        Ok(meta) => meta,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response(),
    };
    let checksum = meta.checksum_hex();
    let algorithm = meta.algorithm_label();

    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        options.version_header,
        HeaderValue::from_str(&version.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    headers.insert(
        options.checksum_header,
        HeaderValue::from_str(&checksum).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        options.checksum_alg_header,
        HeaderValue::from_str(&algorithm).unwrap_or_else(|_| HeaderValue::from_static("sha256")),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn get_metrics(State(state): State<Arc<RelayState>>) -> Response {
    let version = state.version().await;
    let body = format!(
        "# HELP pavis_relay_version Current config version\n# TYPE pavis_relay_version gauge\npavis_relay_version {version}\n"
    );
    (StatusCode::OK, body).into_response()
}

fn format_unix_time(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
