#[path = "../../pavis-e2e/src/support/pvs.rs"]
mod pvs_support;

use crate::routes::router;
use crate::state::{RelayOptions, RelayState};
use axum::body::{Body, Bytes};
use axum::http::HeaderName;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

fn test_state() -> RelayState {
    RelayState::new(7, valid_pvs_bytes("seed")).expect("state")
}

fn test_state_with_options(options: RelayOptions) -> RelayState {
    RelayState::new_with_options(7, valid_pvs_bytes("seed"), options).expect("state")
}

fn valid_pvs_bytes(label: &str) -> Bytes {
    Bytes::from(pvs_support::build_pvs_bytes(label))
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
    let state = RelayState::new(0, Bytes::new()).expect("state");
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
        .oneshot(
            Request::get("/v1/config")
                .header("x-pavis-version", "0")
                .body(Body::empty())
                .unwrap(),
        )
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
    let body_str = std::str::from_utf8(&body).expect("status utf8");
    assert!(body_str.contains("version="));
}

#[tokio::test]
async fn status_reports_unknown_for_empty_meta() {
    let state = RelayState::new(0, Bytes::new()).expect("state");
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
    let body_str = std::str::from_utf8(&body).expect("status utf8");
    assert!(body_str.contains("checksum=invalid"));
}

#[tokio::test]
async fn config_rejects_missing_version_header() {
    let app = router(test_state());

    let response = app
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn config_returns_latest_with_headers() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/config")
                .header("x-pavis-version", "0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("7")
    );
    assert!(
        headers
            .get("x-pavis-checksum")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        headers
            .get("x-pavis-checksum-alg")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
}

#[tokio::test]
async fn config_long_poll_times_out() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/config?wait_ms=1")
                .header("x-pavis-version", "7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn config_long_poll_success() {
    let state = test_state();
    let app = router(state.clone());

    let waiter = tokio::spawn({
        let app = app.clone();
        async move {
            app.oneshot(
                Request::get("/v1/config?wait_ms=5000")
                    .header("x-pavis-version", "7")
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
    assert_eq!(response.headers().get("x-pavis-version").unwrap(), "8");
}

#[tokio::test]
async fn publish_rejects_empty_body() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn publish_requires_version_header() {
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn publish_rejects_invalid_pvs() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::from("bad"))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn publish_rejects_non_monotonic_versions() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "7")
                .body(Body::from(valid_pvs_bytes("same")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn publish_persists_and_serves_latest() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::from(valid_pvs_bytes("next")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::get("/v1/config")
                .header("x-pavis-version", "0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-pavis-version")
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
    let body_str = std::str::from_utf8(&body).expect("status utf8");
    assert!(body_str.contains("checksum="));
    assert!(body_str.contains("checksum_alg="));
}

#[tokio::test]
async fn publish_updates_metrics() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
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
    let mut options = RelayOptions::default();
    options.version_header = HeaderName::from_static("x-test-version");
    options.checksum_header = HeaderName::from_static("x-test-checksum");
    options.checksum_alg_header = HeaderName::from_static("x-test-checksum-alg");
    let app = router(test_state_with_options(options));

    let response = app
        .oneshot(
            Request::get("/v1/config")
                .header("x-test-version", "0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert!(headers.contains_key("x-test-version"));
    assert!(headers.contains_key("x-test-checksum"));
    assert!(headers.contains_key("x-test-checksum-alg"));
}

#[tokio::test]
async fn test_publish_updates_lkg_on_disk() {
    let dir = std::env::temp_dir().join("relay_publish_lkg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let lkg_path = dir.join("config.pvs");

    let mut options = RelayOptions::default();
    options.lkg_path = Some(lkg_path.clone());
    let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");
    let app = router(state);

    let pvs_bytes = valid_pvs_bytes("v2");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/publish")
                .header("x-pavis-version", "1")
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
