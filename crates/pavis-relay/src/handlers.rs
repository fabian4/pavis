//! Relay HTTP handlers for config serving and publishing.
//!
//! This module implements the Relay Config API v1.0 specification with:
//! - ETag-based conditional GET (RFC 9110)
//! - Long-polling with false wakeup protection
//! - Transport integrity headers (`x-config-size`)
//!
//! ## Key Design Decisions
//!
//! ### Two-Level False Wakeup Protection
//! 1. **Source-level**: `publish_*()` methods only notify waiters when ETag/checksum changes
//! 2. **Loop-level**: Long-poll loop re-checks ETag after wake, continues if unchanged
//!
//! This prevents wake storms when identical artifacts are republished frequently.
//!
//! ### Strict ETag Validation
//! - Rejects weak ETags (W/), wildcards (*), multiple ETags
//! - Explicit quote validation (no `trim_matches`)
//! - Normalizes hex to lowercase for matching
//!
//! ### Response Builder Pattern
//! All responses use `Response::builder()` for explicit body construction.
//! This ensures 204/304/503 responses have truly empty bodies.

use crate::runtime::RelayRuntimeState;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use pavis_core::{CONFIG_SIZE_HEADER, CONFIG_VERSION_HEADER};
use std::sync::Arc;
use std::time::Duration;

#[derive(serde::Serialize)]
pub(crate) struct PublishResponse {
    pub(crate) version: u64,
    pub(crate) checksum: String,
    pub(crate) size: u64,
    pub(crate) published_at: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct ConfigQuery {
    pub(crate) wait_ms: Option<u64>,
}

fn parse_if_none_match(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(IF_NONE_MATCH)?;
    let s = value.to_str().ok()?;
    let trimmed = s.trim();

    if trimmed == "*" {
        return None;
    }

    if trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        return None;
    }

    if trimmed.contains(',') {
        return None;
    }

    if !trimmed.starts_with('"') || !trimmed.ends_with('"') || trimmed.len() < 2 {
        return None;
    }

    let unquoted = &trimmed[1..trimmed.len() - 1];
    if unquoted.contains('"') {
        return None;
    }

    if !unquoted.starts_with("sha256:") {
        return None;
    }

    let hex_part = &unquoted[7..];
    if hex_part.len() != 64 {
        return None;
    }

    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(format!("sha256:{}", hex_part.to_lowercase()))
}

fn etag_from_checksum(checksum: &str) -> String {
    let normalized = checksum.to_lowercase();
    if normalized.starts_with("sha256:") {
        normalized
    } else {
        format!("sha256:{normalized}")
    }
}

fn quote_etag(etag: &str) -> String {
    format!("\"{}\"", etag)
}

fn build_200_response(snapshot: crate::runtime::RelaySnapshotView, etag: &str) -> Response {
    let quoted_etag = quote_etag(etag);
    let size = snapshot.pvs_bytes.len();

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(ETAG, quoted_etag)
        .header(CONFIG_SIZE_HEADER, size.to_string())
        .header(CONFIG_VERSION_HEADER, snapshot.version.to_string())
        .header(CACHE_CONTROL, "no-store")
        .body(Body::from(snapshot.pvs_bytes))
        .unwrap()
}

fn build_204_response(etag: &str) -> Response {
    let quoted_etag = quote_etag(etag);
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(ETAG, quoted_etag)
        .header(CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}

fn build_304_response(etag: &str) -> Response {
    let quoted_etag = quote_etag(etag);
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(ETAG, quoted_etag)
        .header(CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}

fn build_503_response() -> Response {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(RETRY_AFTER, "1")
        .body(Body::empty())
        .unwrap()
}

fn build_400_response(message: &str) -> Response {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(format!("{message}\n")))
        .unwrap()
}

pub(crate) async fn get_config(
    State(state): State<Arc<RelayRuntimeState>>,
    Query(query): Query<ConfigQuery>,
    headers: HeaderMap,
) -> Response {
    if !state.is_ready() {
        return build_503_response();
    }

    let options = state.options().clone();
    let wait_ms = query.wait_ms.unwrap_or(0);
    if wait_ms > 60000 {
        return build_400_response("wait_ms must be in range 0..=60000 (milliseconds)");
    }

    let client_etag = parse_if_none_match(&headers);

    let mut snapshot = state.snapshot().await;
    let mut current_etag = etag_from_checksum(&snapshot.artifact_checksum);

    if let Some(ref etag) = client_etag
        && etag != &current_etag
    {
        return build_200_response(snapshot, &current_etag);
    }

    if client_etag.is_none() && wait_ms > 0 {
        return build_200_response(snapshot, &current_etag);
    }

    if wait_ms > 0 && options.long_poll_enabled {
        state.metrics().inc_long_poll_wait();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return build_204_response(&current_etag);
            }

            let notified = state.notifier().notified();
            match tokio::time::timeout(remaining, notified).await {
                Ok(_) => {
                    snapshot = state.snapshot().await;
                    let new_etag = etag_from_checksum(&snapshot.artifact_checksum);
                    if new_etag != current_etag {
                        return build_200_response(snapshot, &new_etag);
                    }
                    current_etag = new_etag;
                }
                Err(_) => {
                    return build_204_response(&current_etag);
                }
            }
        }
    }

    if let Some(ref etag) = client_etag
        && etag == &current_etag
    {
        return build_304_response(&current_etag);
    }

    build_200_response(snapshot, &current_etag)
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
    let storage_root = &options.storage_root;
    let history_count = crate::storage::history::list_history_versions(storage_root)
        .map(|versions| versions.len() as u64)
        .unwrap_or(0);
    let lkg_meta = crate::storage::lkg::load_lkg_metadata(storage_root)
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
    if !state.is_ready() {
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
    use crate::storage::validated_path::ValidatedStorageRoot;
    use axum::body::Bytes;

    async fn response_bytes(response: Response) -> Bytes {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body")
    }

    fn build_valid_pvs_bytes(label: &str) -> Bytes {
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
                service_name: pavis_core::ServiceName(label.to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("config");

        let validated = pavis_core::validate_runtime(config).expect("validate");
        let pvs_bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");
        Bytes::from(pvs_bytes)
    }

    #[tokio::test]
    async fn test_post_publish_failures() {
        let dir = std::env::temp_dir().join("relay_handlers_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let storage_root = ValidatedStorageRoot::new(dir.clone()).expect("validated storage root");
        let options = RelayOptions {
            storage_root,
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
        let valid_body = build_valid_pvs_bytes("pavis");

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
        let fail_storage_root =
            ValidatedStorageRoot::new(fail_dir.clone()).expect("validated storage root");
        let mut fail_opts = options.clone();
        fail_opts.storage_root = fail_storage_root;
        let fail_state = Arc::new(
            RelayRuntimeState::new_with_options(10, Bytes::new(), fail_opts).expect("state"),
        );
        let response = post_publish(State(fail_state), valid_body.clone()).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let _ = std::fs::remove_dir_all(&fail_dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_503_when_not_ready() {
        let state = Arc::new(
            RelayRuntimeState::new_with_options(0, Bytes::new(), RelayOptions::default())
                .expect("state"),
        );
        let response = get_config(
            State(state),
            Query(ConfigQuery { wait_ms: None }),
            HeaderMap::new(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
        let body = response_bytes(response).await;
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_400_for_wait_ms_out_of_range() {
        let pvs_bytes = build_valid_pvs_bytes("range");
        let state = Arc::new(
            RelayRuntimeState::new_with_options(1, pvs_bytes, RelayOptions::default())
                .expect("state"),
        );
        let response = get_config(
            State(state),
            Query(ConfigQuery {
                wait_ms: Some(70000),
            }),
            HeaderMap::new(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_bytes(response).await;
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("wait_ms must be in range 0..=60000"));
    }

    #[tokio::test]
    async fn test_200_unconditional_get() {
        let pvs_bytes = build_valid_pvs_bytes("unconditional");
        let state = Arc::new(
            RelayRuntimeState::new_with_options(1, pvs_bytes.clone(), RelayOptions::default())
                .expect("state"),
        );
        let response = get_config(
            State(state),
            Query(ConfigQuery { wait_ms: None }),
            HeaderMap::new(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/octet-stream"
        );
        assert!(response.headers().get(ETAG).is_some());
        assert!(response.headers().get(CONFIG_SIZE_HEADER).is_some());
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        let body = response_bytes(response).await;
        assert_eq!(body, pvs_bytes);
    }

    #[tokio::test]
    async fn test_304_conditional_get_matching_etag() {
        let pvs_bytes = build_valid_pvs_bytes("conditional");
        let state = Arc::new(
            RelayRuntimeState::new_with_options(1, pvs_bytes, RelayOptions::default())
                .expect("state"),
        );
        let snapshot = state.snapshot().await;
        let etag = quote_etag(&etag_from_checksum(&snapshot.artifact_checksum));
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, etag.parse().unwrap());

        let response = get_config(
            State(state),
            Query(ConfigQuery { wait_ms: Some(0) }),
            headers,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        let body = response_bytes(response).await;
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn test_reject_weak_etag() {
        let pvs_bytes = build_valid_pvs_bytes("weak");
        let state = Arc::new(
            RelayRuntimeState::new_with_options(1, pvs_bytes, RelayOptions::default())
                .expect("state"),
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            "W/\"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\""
                .parse()
                .unwrap(),
        );

        let response =
            get_config(State(state), Query(ConfigQuery { wait_ms: None }), headers).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn parse_if_none_match_accepts_valid_etag() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            "\"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\""
                .parse()
                .unwrap(),
        );
        let result = parse_if_none_match(&headers);
        assert_eq!(
            result,
            Some(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string()
            )
        );
    }

    #[test]
    fn parse_if_none_match_normalizes_uppercase_hex() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            "\"sha256:ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789\""
                .parse()
                .unwrap(),
        );
        let result = parse_if_none_match(&headers);
        assert_eq!(
            result,
            Some(
                "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_string()
            )
        );
    }

    #[test]
    fn parse_if_none_match_rejects_missing_quotes() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, "sha256:abc".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn parse_if_none_match_rejects_weak_etag() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, "W/\"sha256:abc\"".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn parse_if_none_match_rejects_multiple() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, "\"etag1\", \"etag2\"".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn parse_if_none_match_rejects_wildcard() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, "*".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }
}
