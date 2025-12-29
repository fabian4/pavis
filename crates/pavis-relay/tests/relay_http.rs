use axum::body::Body;
use axum::body::Bytes;
use axum::http::{Request, StatusCode};
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
