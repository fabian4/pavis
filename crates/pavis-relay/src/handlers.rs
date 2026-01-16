use crate::runtime::RelayRuntimeState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

const CONFIG_CHECKSUM_HEADER: &str = "x-config-checksum";
const CONFIG_SIZE_HEADER: &str = "x-config-size";
const CONFIG_VERSION_HEADER: &str = "x-config-version";

#[derive(serde::Serialize)]
pub(crate) struct PublishResponse {
    pub(crate) version: u64,
    pub(crate) checksum: String,
    pub(crate) size: u64,
    pub(crate) published_at: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct ConfigQuery {
    pub(crate) timeout: Option<u64>,
}

pub(crate) async fn get_config(
    State(state): State<Arc<RelayRuntimeState>>,
    Query(query): Query<ConfigQuery>,
) -> Response {
    let options = state.options().clone();
    let timeout = query.timeout.unwrap_or(30);
    if !(1..=60).contains(&timeout) {
        return (StatusCode::BAD_REQUEST, "timeout must be within [1, 60]\n").into_response();
    }
    if options.long_poll_enabled {
        state.metrics().inc_long_poll_wait();
        let notified = state.notifier().notified();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(timeout), notified).await;
    }

    let snapshot = state.snapshot().await;
    let checksum = snapshot.artifact_checksum;

    let size = snapshot.pvs_bytes.len();
    let mut response = snapshot.pvs_bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        CONFIG_VERSION_HEADER,
        HeaderValue::from_str(&snapshot.version.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    headers.insert(
        CONFIG_CHECKSUM_HEADER,
        HeaderValue::from_str(&checksum).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        CONFIG_SIZE_HEADER,
        HeaderValue::from_str(&size.to_string()).unwrap_or_else(|_| HeaderValue::from_static("0")),
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

pub(crate) async fn get_status(State(state): State<Arc<RelayRuntimeState>>) -> Response {
    #[derive(serde::Serialize)]
    struct StatusLkg {
        version: u64,
        size: u64,
        checksum: String,
        published_at: String,
    }

    #[derive(serde::Serialize)]
    struct StatusResponse {
        status: &'static str,
        uptime_s: u64,
        current_version: u64,
        lkg: Option<StatusLkg>,
        history_count: u64,
    }

    let options = state.options();
    let uptime_seconds = uptime_seconds(state.started_at());
    let current_version = state.version().await;
    let storage_root = options.storage_root.clone();
    let history_count = crate::storage::history::list_history_versions(&storage_root)
        .map(|versions| versions.len() as u64)
        .unwrap_or(0);
    let lkg_meta = crate::storage::lkg::load_lkg_metadata(&storage_root)
        .ok()
        .flatten();
    let lkg = lkg_meta.map(|meta| StatusLkg {
        version: meta.version,
        size: meta.size,
        checksum: meta.checksum,
        published_at: chrono::DateTime::<chrono::Utc>::from(meta.published_at).to_rfc3339(),
    });

    let body = StatusResponse {
        status: "healthy",
        uptime_s: uptime_seconds,
        current_version,
        lkg,
        history_count,
    };
    (StatusCode::OK, axum::Json(body)).into_response()
}

pub(crate) async fn get_health() -> Response {
    (StatusCode::OK, "ok\n").into_response()
}

pub(crate) async fn get_ready(State(state): State<Arc<RelayRuntimeState>>) -> Response {
    let snapshot = state.snapshot().await;
    if snapshot.pvs_bytes.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "no artifact\n").into_response();
    }
    (StatusCode::OK, "ready\n").into_response()
}

pub(crate) async fn post_publish(
    State(state): State<Arc<RelayRuntimeState>>,
    body: Bytes,
) -> Response {
    if body.is_empty() {
        state.metrics().inc_publish_fail();
        return (StatusCode::BAD_REQUEST, "request body is empty\n").into_response();
    }

    let metadata = match state.publish_bytes(body).await {
        Ok(metadata) => metadata,
        Err(err) => {
            state.metrics().inc_publish_fail();
            state.set_last_error(Some(err.to_string())).await;
            let status = match err {
                crate::runtime::RelayError::Policy(_) => StatusCode::PAYLOAD_TOO_LARGE,
                crate::runtime::RelayError::Config(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            return (status, format!("{err}\n")).into_response();
        }
    };

    state.set_last_error(None).await;
    let response = PublishResponse {
        version: metadata.version,
        checksum: metadata.checksum,
        size: metadata.size,
        published_at: chrono::DateTime::<chrono::Utc>::from(metadata.published_at).to_rfc3339(),
    };

    (StatusCode::OK, axum::Json(response)).into_response()
}

pub(crate) async fn get_artifact(
    State(state): State<Arc<RelayRuntimeState>>,
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

pub(crate) async fn get_metrics(State(state): State<Arc<RelayRuntimeState>>) -> Response {
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

fn uptime_seconds(started_at: std::time::SystemTime) -> u64 {
    std::time::SystemTime::now()
        .duration_since(started_at)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RelayOptions;
    use axum::body::Bytes;

    #[tokio::test]
    async fn test_post_publish_failures() {
        let dir = std::env::temp_dir().join("relay_handlers_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let options = RelayOptions {
            storage_root: dir.clone(),
            max_pvs_bytes: 1000,
            ..Default::default()
        };

        let state = Arc::new(
            RelayRuntimeState::new_with_options(10, Bytes::new(), options.clone()).expect("state"),
        );

        // 1. Verification Failure
        let body = Bytes::from_static(b"invalid pvs data");

        let response = post_publish(State(state.clone()), body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Prepare valid PVS
        let listener = pavis_core::ListenerBuilder::new()
            .name(pavis_core::ListenerName("default".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .workers(pavis_core::WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: pavis_core::ServiceName("pavis".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("config");
        let validated = pavis_core::validate_runtime(config).expect("validate");
        let pvs_bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");
        let valid_body = Bytes::from(pvs_bytes);

        // 2. Policy Failure (max_pvs_bytes)
        // Create new state with small limit
        let mut small_opts = options.clone();
        small_opts.max_pvs_bytes = 10;
        let small_state = Arc::new(
            RelayRuntimeState::new_with_options(10, Bytes::new(), small_opts).expect("state"),
        );
        let response = post_publish(State(small_state), valid_body.clone()).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        // 3. LKG Write Failure
        let fail_dir = std::env::temp_dir().join("relay_handlers_fail");
        let _ = std::fs::remove_dir_all(&fail_dir);
        std::fs::create_dir_all(&fail_dir).unwrap();
        std::fs::write(fail_dir.join("lkg"), b"block").unwrap();
        let mut fail_opts = options.clone();
        fail_opts.storage_root = fail_dir.clone();
        let fail_state = Arc::new(
            RelayRuntimeState::new_with_options(10, Bytes::new(), fail_opts).expect("state"),
        );
        let response = post_publish(State(fail_state), valid_body.clone()).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let _ = std::fs::remove_dir_all(&fail_dir);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
