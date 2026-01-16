use crate::integration::support::{state_with_storage, valid_pvs_bytes};
use crate::routes::router;
use crate::storage::lkg::load_lkg_metadata;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

#[derive(serde::Deserialize)]
struct PublishResponse {
    version: u64,
    checksum: String,
    size: u64,
    published_at: String,
}

async fn publish_once(app: &axum::Router, bytes: Bytes) -> (StatusCode, Option<PublishResponse>) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/publish")
                .body(Body::from(bytes))
                .expect("request"),
        )
        .await
        .expect("publish");
    let status = response.status();
    if status != StatusCode::OK {
        return (status, None);
    }
    let body = response
        .into_body()
        .collect()
        .await
        .expect("publish body")
        .to_bytes();
    let json = serde_json::from_slice::<PublishResponse>(&body).expect("publish json");
    (status, Some(json))
}

#[tokio::test]
async fn publish_monotonic_versions_and_persists_lkg_metadata() {
    let bytes = valid_pvs_bytes("publish_monotonic");
    let state = state_with_storage("publish_monotonic", 0, Bytes::new());
    let storage_root = state.options().storage_root.clone();
    let app = router(state.clone());

    let (_, first) = publish_once(&app, bytes.clone()).await;
    let (_, second) = publish_once(&app, bytes.clone()).await;

    let first = first.expect("first publish");
    let second = second.expect("second publish");

    assert_eq!(first.version, 1);
    assert_eq!(second.version, 2);
    assert_eq!(state.version().await, 2);

    let lkg_meta = load_lkg_metadata(&storage_root)
        .expect("load lkg metadata")
        .expect("lkg metadata");
    assert_eq!(lkg_meta.version, second.version);
    assert_eq!(lkg_meta.checksum, second.checksum);
    assert_eq!(lkg_meta.size, second.size);
}

#[tokio::test]
async fn publish_idempotency_returns_distinct_versions() {
    let bytes = valid_pvs_bytes("publish_idempotent");
    let state = state_with_storage("publish_idempotent", 0, Bytes::new());
    let app = router(state);

    let (_, first) = publish_once(&app, bytes.clone()).await;
    let (_, second) = publish_once(&app, bytes.clone()).await;

    let first = first.expect("first publish");
    let second = second.expect("second publish");

    assert_ne!(first.version, second.version);
    assert_eq!(first.checksum, second.checksum);
    assert_eq!(first.size, second.size);
    assert!(!first.published_at.is_empty());
}

#[tokio::test]
async fn publish_invalid_pvs_returns_bad_request() {
    let state = state_with_storage("publish_invalid", 0, Bytes::new());
    let app = router(state);

    let (status, body) = publish_once(&app, Bytes::from_static(b"not a pvs")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.is_none());
}
