use crate::state::{RelayMeta, RelayState};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use pavis_pvs::{VerifiedPvs, verify};
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
        state.metrics().inc_long_poll_wait();
        let notified = state.notifier().notified();
        let _ = tokio::time::timeout(std::time::Duration::from_millis(wait_ms), notified).await;
        let latest_version = state.version().await;
        if latest_version == current_version {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    let snapshot = state.snapshot().await;
    let checksum = snapshot.meta.checksum.clone();
    let algorithm = snapshot.meta.algorithm.clone();

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
    let generated_at = chrono::DateTime::<chrono::Utc>::from(snapshot.updated_at).to_rfc3339();
    headers.insert(
        options.generated_at_header,
        HeaderValue::from_str(&generated_at).unwrap_or_else(|_| HeaderValue::from_static("")),
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
    let checksum = if snapshot.meta.checksum.is_empty() {
        "invalid".to_string()
    } else {
        snapshot.meta.checksum.clone()
    };
    let algorithm = if snapshot.meta.algorithm.is_empty() {
        "unknown".to_string()
    } else {
        snapshot.meta.algorithm.clone()
    };
    let uptime_seconds = uptime_seconds(state.started_at());
    let last_update_unix_ms = unix_millis(snapshot.updated_at);
    let body = format!(
        "name={} version={version} checksum={checksum} checksum_alg={algorithm} size={size} uptime_seconds={uptime_seconds} last_update_unix_ms={last_update_unix_ms}\n",
        options.identity_name,
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
        state.metrics().inc_publish_fail();
        return (StatusCode::BAD_REQUEST, "empty body\n").into_response();
    }
    let proposed_version = match headers
        .get(&options.version_header)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(version) => version,
        None => {
            state.metrics().inc_publish_fail();
            return (
                StatusCode::BAD_REQUEST,
                format!("missing {}\n", options.version_header.as_str()),
            )
                .into_response();
        }
    };

    let verified = match verify(&body) {
        Ok(verified) => verified,
        Err(err) => {
            state.metrics().inc_publish_fail();
            state.set_last_error(Some(err.to_string())).await;
            return (StatusCode::UNPROCESSABLE_ENTITY, format!("{err}\n")).into_response();
        }
    };

    let (payload, meta) = verified_payload(verified);
    if let Err(err) = state
        .publish(proposed_version, payload.clone(), meta.clone())
        .await
    {
        state.metrics().inc_publish_fail();
        state.set_last_error(Some(err.to_string())).await;
        let status = match err {
            crate::state::RelayError::Policy(_) => StatusCode::PAYLOAD_TOO_LARGE,
            crate::state::RelayError::VersionMonotonicity { .. } => StatusCode::CONFLICT,
            _ => StatusCode::CONFLICT,
        };
        return (status, format!("{err}\n")).into_response();
    }
    state.metrics().inc_publish_ok();
    state.set_last_error(None).await;
    if let Some(path) = options.lkg_path.as_ref() {
        if let Some(parent) = path.parent() {
            match tokio::fs::create_dir_all(parent).await {
                Ok(()) => {}
                Err(err) => {
                    state.metrics().inc_publish_fail();
                    state.set_last_error(Some(err.to_string())).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response();
                }
            }
        }
        if let Err(err) = tokio::fs::write(path, &payload).await {
            state.metrics().inc_publish_fail();
            state.set_last_error(Some(err.to_string())).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}\n")).into_response();
        }
    }

    let checksum = meta.checksum;
    let algorithm = meta.algorithm;

    let mut response = (StatusCode::OK, "ok\n").into_response();
    let headers = response.headers_mut();
    headers.insert(
        options.version_header,
        HeaderValue::from_str(&proposed_version.to_string())
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
    response
}

pub(crate) async fn get_artifact(
    State(state): State<Arc<RelayState>>,
    Path(version): Path<u64>,
) -> Response {
    let options = state.options().clone();
    let artifact = match state.artifact(version).await {
        Some(artifact) => artifact,
        None => return (StatusCode::NOT_FOUND, "unknown version\n").into_response(),
    };
    let checksum = artifact.meta.checksum;
    let algorithm = artifact.meta.algorithm;

    let mut response = artifact.bytes.into_response();
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
    let generated_at = chrono::DateTime::<chrono::Utc>::from(artifact.generated_at).to_rfc3339();
    headers.insert(
        options.generated_at_header,
        HeaderValue::from_str(&generated_at).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn get_metrics(State(state): State<Arc<RelayState>>) -> Response {
    let version = state.version().await;
    let metrics = state.metrics();
    let body = format!(
        "# HELP pavis_relay_version Current config version\n# TYPE pavis_relay_version gauge\npavis_relay_version {version}\n\
# HELP pavis_relay_publish_ok_total Successful publishes\n# TYPE pavis_relay_publish_ok_total counter\npavis_relay_publish_ok_total {}\n\
# HELP pavis_relay_publish_fail_total Failed publishes\n# TYPE pavis_relay_publish_fail_total counter\npavis_relay_publish_fail_total {}\n\
# HELP pavis_relay_longpoll_wait_total Long poll waits\n# TYPE pavis_relay_longpoll_wait_total counter\npavis_relay_longpoll_wait_total {}\n",
        metrics.publish_ok(),
        metrics.publish_fail(),
        metrics.long_poll_wait()
    );
    (StatusCode::OK, body).into_response()
}

fn verified_payload(verified: VerifiedPvs) -> (Bytes, RelayMeta) {
    let meta = RelayMeta {
        checksum: verified.checksum_hex(),
        algorithm: verified.algorithm_label(),
        schema_version: verified.version(),
    };
    (Bytes::from(verified.into_bytes()), meta)
}

fn uptime_seconds(started_at: std::time::SystemTime) -> u64 {
    std::time::SystemTime::now()
        .duration_since(started_at)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_millis(time: std::time::SystemTime) -> u128 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RelayOptions;
    use axum::body::Bytes;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn test_post_publish_failures() {
        let dir = std::env::temp_dir().join("relay_handlers_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lkg = dir.join("config.pvs");

        let mut options = RelayOptions::default();
        options.lkg_path = Some(lkg.clone());
        options.max_pvs_bytes = 1000; // ample limit initially

        let state = Arc::new(
            RelayState::new_with_options(10, Bytes::new(), options.clone()).expect("state"),
        );

        // 1. Verification Failure
        let mut headers = HeaderMap::new();
        headers.insert(options.version_header.clone(), "11".parse().unwrap());
        let body = Bytes::from_static(b"invalid pvs data");

        let response = post_publish(State(state.clone()), headers.clone(), body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Prepare valid PVS
        let config = pavis_core::RuntimeConfig {
            listeners: vec![pavis_core::Listener {
                name: pavis_core::ListenerName("default".to_string()),
                address: "127.0.0.1:8080".parse().unwrap(),
                workers: pavis_core::WorkerCount::Auto,
                tls: pavis_core::TlsConfig::Disabled,
            }],
            telemetry: pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: pavis_core::ServiceName("pavis".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            },
            upstreams: vec![],
            routes: vec![],
        };
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");
        let valid_body = Bytes::from(pvs_bytes);

        // 2. Policy Failure (max_pvs_bytes)
        // Create new state with small limit
        let mut small_opts = options.clone();
        small_opts.max_pvs_bytes = 10;
        let small_state =
            Arc::new(RelayState::new_with_options(10, Bytes::new(), small_opts).expect("state"));
        let response = post_publish(State(small_state), headers.clone(), valid_body.clone()).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        // 3. Monotonicity Failure
        // Try to publish version 10 when current is 10
        let mut bad_ver_headers = HeaderMap::new();
        bad_ver_headers.insert(options.version_header.clone(), "10".parse().unwrap());
        let response =
            post_publish(State(state.clone()), bad_ver_headers, valid_body.clone()).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);

        // 4. LKG Write Failure
        // Use a directory as LKG path
        let fail_lkg = dir.join("fail_lkg_dir");
        std::fs::create_dir(&fail_lkg).unwrap();
        let mut fail_opts = options.clone();
        fail_opts.lkg_path = Some(fail_lkg);
        let fail_state =
            Arc::new(RelayState::new_with_options(10, Bytes::new(), fail_opts).expect("state"));

        let mut good_headers = HeaderMap::new();
        good_headers.insert(options.version_header.clone(), "11".parse().unwrap());

        let response = post_publish(State(fail_state), good_headers, valid_body.clone()).await;
        // On Unix, writing to directory is IsADirectory (OS error 21). On Windows, AccessDenied.
        // It returns INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
