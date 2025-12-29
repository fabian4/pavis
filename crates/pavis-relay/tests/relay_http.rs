use axum::body::Body;
use axum::body::Bytes;
use axum::http::HeaderName;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pavis_pvs::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, compute_checksum,
};
use pavis_relay::{RelayOptions, RelayState, router};
use tower::util::ServiceExt;

fn valid_pvs_bytes(payload: &[u8]) -> Bytes {
    let checksum = compute_checksum(payload);
    let mut bytes = Vec::with_capacity(HEADER_SIZE + payload.len());
    bytes.extend_from_slice(PAVIS_MAGIC);
    bytes.extend_from_slice(&PAVIS_VERSION.to_le_bytes());
    bytes.extend_from_slice(&PAVIS_HASH_ALGORITHM_SHA256.to_le_bytes());
    bytes.extend_from_slice(&checksum);
    bytes.extend_from_slice(&[0u8; 20]);
    bytes.extend_from_slice(payload);
    Bytes::from(bytes)
}

fn test_state() -> RelayState {
    RelayState::new(7, valid_pvs_bytes(b"seed")).expect("state")
}

fn test_state_with_options(options: RelayOptions) -> RelayState {
    RelayState::new_with_options(7, valid_pvs_bytes(b"seed"), options).expect("state")
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
        .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
        .await
        .expect("ready");
    assert_eq!(response.status(), StatusCode::OK);
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
}

#[tokio::test]
async fn publish_and_fetch_artifact() {
    let app = router(test_state());

    let bytes = Bytes::from_static(b"next");

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::from(valid_pvs_bytes(&bytes)))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::get("/v1/artifacts/8").body(Body::empty()).unwrap())
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_long_poll_returns_not_modified() {
    let app = router(test_state());

    let response = app
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
async fn publish_requires_version_header() {
    let app = router(test_state());

    let response = app
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(Bytes::from_static(b"next")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn publish_rejects_non_increasing_version() {
    let app = router(test_state());

    let response = app
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "7")
                .body(Body::from(valid_pvs_bytes(b"same")))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn artifact_and_metrics_bodies_are_stable() {
    let app = router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/artifacts/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("artifact");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("artifact body")
        .to_bytes();
    assert_eq!(body, Bytes::from_static(b"unknown version\n"));

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
    assert!(body_str.contains("pavis_relay_version"));
    assert!(body_str.contains("pavis_relay_version 7"));
}

#[tokio::test]
async fn config_long_poll_returns_update_with_headers() {
    let app = router(test_state());

    let wait_handle = tokio::spawn({
        let app = app.clone();
        async move {
            app.oneshot(
                Request::get("/v1/config?wait_ms=2000")
                    .header("x-pavis-version", "7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let expected = valid_pvs_bytes(b"next");
    let publish_response = app
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::from(expected.clone()))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(publish_response.status(), StatusCode::OK);

    let response = wait_handle.await.expect("wait join").expect("wait request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        response
            .headers()
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("8")
    );
    let body = response
        .into_body()
        .collect()
        .await
        .expect("config body")
        .to_bytes();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn publish_then_fetch_config_returns_latest_bytes() {
    let app = router(test_state());

    let expected = valid_pvs_bytes(b"fresh");
    let publish_response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "9")
                .body(Body::from(expected.clone()))
                .unwrap(),
        )
        .await
        .expect("publish");
    assert_eq!(publish_response.status(), StatusCode::OK);

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
    assert_eq!(
        response
            .headers()
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("9")
    );
    let body = response
        .into_body()
        .collect()
        .await
        .expect("config body")
        .to_bytes();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn publish_twice_preserves_prior_artifact_as_lkg() {
    let app = router(test_state());

    let expected = valid_pvs_bytes(b"first");
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "9")
                .body(Body::from(expected.clone()))
                .unwrap(),
        )
        .await
        .expect("publish first");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "10")
                .body(Body::from(valid_pvs_bytes(b"second")))
                .unwrap(),
        )
        .await
        .expect("publish second");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::get("/v1/artifacts/9").body(Body::empty()).unwrap())
        .await
        .expect("artifact 9");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("artifact body")
        .to_bytes();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn publish_same_version_keeps_original_config() {
    let app = router(test_state());

    let expected = valid_pvs_bytes(b"first");
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "9")
                .body(Body::from(expected.clone()))
                .unwrap(),
        )
        .await
        .expect("publish first");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "9")
                .body(Body::from(valid_pvs_bytes(b"second")))
                .unwrap(),
        )
        .await
        .expect("publish same");
    assert_eq!(response.status(), StatusCode::CONFLICT);

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
    let body = response
        .into_body()
        .collect()
        .await
        .expect("config body")
        .to_bytes();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn config_uses_custom_header_names_and_status_identity() {
    let options = RelayOptions {
        version_header: HeaderName::from_static("x-test-version"),
        checksum_header: HeaderName::from_static("x-test-checksum"),
        checksum_alg_header: HeaderName::from_static("x-test-checksum-alg"),
        long_poll_enabled: false,
        identity_name: "relay-a".to_string(),
    };
    let app = router(test_state_with_options(options));

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/config")
                .header("x-test-version", "7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-test-version"));
    assert!(response.headers().contains_key("x-test-checksum"));
    assert!(response.headers().contains_key("x-test-checksum-alg"));

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
    assert!(body_str.contains("name=relay-a"));
}
