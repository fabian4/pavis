use crate::integration::support::{state_with_storage, valid_pvs_bytes};
use crate::routes::router;
use crate::storage::metadata::checksum_for_bytes;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

async fn publish(app: &axum::Router, bytes: Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(bytes))
                .expect("publish request"),
        )
        .await
        .expect("publish");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_serves_lkg_bytes_with_checksum_headers() {
    let bytes = valid_pvs_bytes("config_headers");
    let state = state_with_storage("config_headers", 0, Bytes::new());
    let app = router(state);

    publish(&app, bytes.clone()).await;

    let response = app
        .oneshot(
            Request::get("/v1/config?timeout=1")
                .body(Body::empty())
                .expect("config request"),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);

    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("config body")
        .to_bytes();

    let checksum = checksum_for_bytes(&body);
    let size = body.len().to_string();
    assert_eq!(
        headers
            .get("x-config-checksum")
            .and_then(|value| value.to_str().ok()),
        Some(checksum.as_str())
    );
    assert_eq!(
        headers
            .get("x-config-size")
            .and_then(|value| value.to_str().ok()),
        Some(size.as_str())
    );
}

#[tokio::test]
async fn config_version_header_matches_publish_version() {
    let bytes = valid_pvs_bytes("config_version");
    let state = state_with_storage("config_version", 0, Bytes::new());
    let app = router(state);

    publish(&app, bytes.clone()).await;

    let response = app
        .oneshot(
            Request::get("/v1/config?timeout=1")
                .body(Body::empty())
                .expect("config request"),
        )
        .await
        .expect("config");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-config-version")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
}
