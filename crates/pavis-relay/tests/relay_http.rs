use axum::body::Body;
use axum::body::Bytes;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pavis_relay::{RelayState, router};
use tower::util::ServiceExt;

fn test_state() -> RelayState {
    RelayState::new(7, Bytes::from_static(b"pvs")).expect("state")
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
        .oneshot(Request::get("/v1/config").body(Body::empty()).unwrap())
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
                .body(Body::from(bytes))
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
                .body(Body::from(Bytes::from_static(b"same")))
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

    let publish_response = app
        .oneshot(
            Request::post("/v1/publish")
                .header("x-pavis-version", "8")
                .body(Body::from(Bytes::from_static(b"next")))
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
    assert_eq!(body, Bytes::from_static(b"next"));
}
