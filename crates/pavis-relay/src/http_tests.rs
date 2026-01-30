use crate::routes::router;
use crate::runtime::{RelayOptions, RelayRuntimeState};
use crate::storage::validated_path::ValidatedStorageRoot;
use axum::body::{Body, Bytes};
use axum::http::HeaderName;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

fn build_pvs_bytes(label: &str) -> Vec<u8> {
    use pavis_core::{
        AccessLogPolicy, ListenerName, Metrics, RuntimeConfigBuilder, ServiceName, Telemetry,
        TlsConfig, TracingPolicy, WorkerCount,
    };

    let listener = pavis_core::ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address("127.0.0.1:8080".parse().expect("addr"))
        .workers(WorkerCount::Auto)
        .tls(TlsConfig::Disabled)
        .build()
        .expect("listener");

    let config = RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(label.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config");

    let dir = std::env::temp_dir();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let path = dir.join(format!("pavis_pvs_helper_{label}_{pid}_{id}.pvs"));
    pavis_pvs::write(&path, &config).expect("write config");
    let bytes = std::fs::read(&path).expect("read config");
    let _ = std::fs::remove_file(&path);
    bytes
}

fn test_state() -> RelayRuntimeState {
    let storage_root =
        ValidatedStorageRoot::new(temp_storage_root("seed")).expect("validated storage root");
    let options = RelayOptions {
        storage_root,
        ..Default::default()
    };
    RelayRuntimeState::new_with_options(7, valid_pvs_bytes("seed"), options).expect("state")
}

fn test_state_with_options(options: RelayOptions) -> RelayRuntimeState {
    RelayRuntimeState::new_with_options(7, valid_pvs_bytes("seed"), options).expect("state")
}

fn valid_pvs_bytes(label: &str) -> Bytes {
    Bytes::from(build_pvs_bytes(label))
}

fn temp_storage_root(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.join(format!("relay_storage_{label}_{pid}_{nanos}"))
}

#[tokio::test]
async fn health_and_ready_endpoints_ok() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .expect("health");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
        .await
        .expect("ready");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_unavailable_when_empty() {
    let state = RelayRuntimeState::new(0, Bytes::new()).expect("state");
    let app = router(state);

    let response = app
        .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
        .await
        .expect("ready");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn config_and_status_endpoints_ok() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::get("/v1/status").body(Body::empty()).unwrap())
        .await
        .expect("status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("status body")
        .to_bytes();
    let body_json: serde_json::Value = serde_json::from_slice(&body).expect("status json");
    assert_eq!(
        body_json.get("status").and_then(|value| value.as_str()),
        Some("healthy")
    );
    assert_eq!(
        body_json
            .get("current_version")
            .and_then(|value| value.as_u64()),
        Some(7)
    );
}

#[tokio::test]
async fn status_reports_unknown_for_empty_meta() {
    let state = RelayRuntimeState::new(0, Bytes::new()).expect("state");
    let app = router(state);

    let response = app
        .oneshot(Request::get("/v1/status").body(Body::empty()).unwrap())
        .await
        .expect("status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("status body")
        .to_bytes();
    let body_json: serde_json::Value = serde_json::from_slice(&body).expect("status json");
    assert!(body_json.get("lkg").is_some());
    assert!(body_json.get("lkg").unwrap().is_null());
}

#[tokio::test]
async fn config_rejects_invalid_timeout() {
    let app = router(test_state());

    let response = app
        .oneshot(
            Request::get("/v1/config?wait_ms=70000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn config_returns_latest_with_headers() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-config-version")
            .and_then(|value| value.to_str().ok()),
        Some("7")
    );
    assert!(
        headers
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        headers
            .get("x-config-size")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

#[tokio::test]
async fn config_long_poll_times_out() {
    let app = router(test_state());

    let initial = app
        .clone()
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    let etag = initial
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .expect("etag");

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/config?wait_ms=1")
                .header("if-none-match", etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn config_long_poll_success() {
    let state = test_state();
    let app = router(state.clone());

    let initial = app
        .clone()
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    let etag = initial
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .expect("etag")
        .to_string();

    let waiter = tokio::spawn({
        let app = app.clone();
        let etag = etag.clone();
        async move {
            app.oneshot(
                Request::get("/v1/config?wait_ms=5000")
                    .header("if-none-match", etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Publish update
    state.publish_auto(valid_pvs_bytes("update")).await.unwrap();

    let response = waiter.await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("x-config-version").unwrap(), "8");
}

#[tokio::test]
async fn publish_rejects_empty_body() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(Request::post("/v1/publish").body(Body::empty()).unwrap())
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn publish_accepts_without_version_header() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(valid_pvs_bytes("next")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn publish_rejects_invalid_pvs() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from("bad"))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn publish_persists_and_serves_latest() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(valid_pvs_bytes("next")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-config-version")
            .and_then(|value| value.to_str().ok()),
        Some("8")
    );
}

#[tokio::test]
async fn artifact_endpoint_returns_bytes() {
    let state = test_state();
    let app = router(state.clone());

    let response = app
        .oneshot(Request::get("/v1/artifacts/7").body(Body::empty()).unwrap())
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn artifact_endpoint_returns_exact_bytes() {
    let bytes = valid_pvs_bytes("opaque");
    let state = RelayRuntimeState::new(7, bytes.clone()).expect("state");
    let app = router(state);

    let response = app
        .oneshot(Request::get("/v1/artifacts/7").body(Body::empty()).unwrap())
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("artifact body")
        .to_bytes();
    assert_eq!(body, bytes);
}

#[tokio::test]
async fn artifact_endpoint_returns_404() {
    let app = router(test_state());

    let response = app
        .oneshot(
            Request::get("/v1/artifacts/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn status_includes_checksum_headers() {
    let app = router(test_state());

    let response = app
        .oneshot(Request::get("/v1/status").body(Body::empty()).unwrap())
        .await
        .expect("status");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("status body")
        .to_bytes();
    let body_json: serde_json::Value = serde_json::from_slice(&body).expect("status json");
    assert!(body_json.get("current_version").is_some());
    assert!(body_json.get("history_count").is_some());
}

#[tokio::test]
async fn publish_updates_metrics() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(valid_pvs_bytes("next")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::get("/v1/metrics").body(Body::empty()).unwrap())
        .await
        .expect("metrics");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("metrics body")
        .to_bytes();
    let body_str = std::str::from_utf8(&body).expect("metrics utf8");
    assert!(body_str.contains("pavis_relay_publish_ok_total"));
    assert!(body_str.contains("pavis_relay_publish_fail_total"));
    assert!(body_str.contains("pavis_relay_longpoll_wait_total"));
}

#[tokio::test]
async fn custom_headers_override_defaults() {
    let storage_root = ValidatedStorageRoot::new(temp_storage_root("custom_headers"))
        .expect("validated storage root");
    let options = RelayOptions {
        version_header: HeaderName::from_static("x-test-version"),
        checksum_header: HeaderName::from_static("x-test-checksum"),
        checksum_alg_header: HeaderName::from_static("x-test-checksum-alg"),
        storage_root,
        ..Default::default()
    };
    let app = router(test_state_with_options(options));

    let response = app
        .oneshot(Request::get("/v1/artifacts/7").body(Body::empty()).unwrap())
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert!(headers.contains_key("x-test-version"));
    assert!(headers.contains_key("x-test-checksum"));
    assert!(headers.contains_key("x-test-checksum-alg"));
}

#[tokio::test]
async fn test_publish_updates_lkg_on_disk() {
    let dir = std::env::temp_dir().join(format!(
        "relay_publish_lkg_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let storage_root = ValidatedStorageRoot::new(dir.clone()).expect("validated storage root");
    let lkg_path = crate::storage::lkg::lkg_artifact_path(&storage_root);
    std::fs::create_dir_all(lkg_path.parent().unwrap()).unwrap();

    let options = RelayOptions {
        storage_root,
        ..Default::default()
    };
    let state = RelayRuntimeState::new_with_options(0, Bytes::new(), options).expect("state");
    let app = router(state);

    let pvs_bytes = valid_pvs_bytes("v2");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/publish")
                .body(Body::from(pvs_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(lkg_path.exists());
    let saved_bytes = std::fs::read(&lkg_path).unwrap();
    assert_eq!(saved_bytes, pvs_bytes.to_vec());
    let _ = std::fs::remove_dir_all(&dir);
}
