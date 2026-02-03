mod common;

use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use common::*;
use pavis::agent::PollOutcome;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis_core::ConfigVersion;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn apply_update_removes_tmp_on_load_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg.clone(), state);

    let bad_pvs = vec![0u8; 100];
    let checksum = etag_for_bytes(&bad_pvs);
    let err = agent
        .apply_update_for_tests(bad_pvs, checksum, None)
        .await
        .expect_err("verify failure");
    assert!(err.to_string().contains("magic"));
}

#[tokio::test]
async fn poll_once_treats_500_as_transient_unavailable() {
    let app = Router::new().route(
        "/v1/config",
        get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, PollOutcome::NoChange));
}

#[tokio::test]
async fn apply_update_rejects_etag_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");
    write_pvs(&lkg, "v1");

    let config = minimal_config("v2");
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg.clone(), state_handle);
    let bytes = std::fs::read(&lkg).expect("read");
    let err = agent
        .apply_update_for_tests(bytes, "sha256:bad".to_string(), None)
        .await
        .expect_err("etag mismatch");
    assert!(err.to_string().contains("etag/sha256 mismatch"));
}

#[tokio::test]
async fn poll_once_missing_etag_header() {
    let app = Router::new().route("/v1/config", get(|| async { StatusCode::OK }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let err = agent.poll_once(0).await.expect_err("should fail");
    assert!(err.to_string().contains("missing etag response header"));
}

#[tokio::test]
async fn poll_once_rejected_etag_triggers_304_not_200() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let bad_bytes = Arc::new(vec![0u8; 64]);
    let bad_etag = etag_for_bytes(bad_bytes.as_ref());
    let responses_200 = Arc::new(AtomicUsize::new(0));
    let responses_304 = Arc::new(AtomicUsize::new(0));

    let app = {
        let bad_bytes_outer = Arc::clone(&bad_bytes);
        let bad_etag_outer = bad_etag.clone();
        let responses_200_outer = Arc::clone(&responses_200);
        let responses_304_outer = Arc::clone(&responses_304);
        Router::new().route(
            "/v1/config",
            get(move |headers: axum::http::HeaderMap| {
                let bad_bytes_inner = Arc::clone(&bad_bytes_outer);
                let bad_etag_inner = bad_etag_outer.clone();
                let responses_200_inner = Arc::clone(&responses_200_outer);
                let responses_304_inner = Arc::clone(&responses_304_outer);
                async move {
                    use axum::response::Response;
                    let header_match = headers
                        .get(axum::http::header::IF_NONE_MATCH)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.trim_matches('"').to_string());
                    if header_match.as_deref() == Some(bad_etag_inner.as_str()) {
                        responses_304_inner.fetch_add(1, Ordering::SeqCst);
                        return StatusCode::NOT_MODIFIED.into_response();
                    }

                    responses_200_inner.fetch_add(1, Ordering::SeqCst);
                    let mut response =
                        Response::new(axum::body::Body::from(bad_bytes_inner.as_ref().clone()));
                    *response.status_mut() = StatusCode::OK;
                    response.headers_mut().insert(
                        axum::http::header::ETAG,
                        axum::http::HeaderValue::from_str(&format!("\"{}\"", bad_etag_inner))
                            .unwrap(),
                    );
                    response
                }
            }),
        )
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());

    let outcome = agent.poll_once(0).await.expect("poll 1");
    assert!(!matches!(outcome, PollOutcome::Updated));
    assert_eq!(agent.last_rejected_etag_for_tests(), Some(bad_etag.clone()));

    let outcome = agent.poll_once(0).await.expect("poll 2");
    assert!(matches!(outcome, PollOutcome::NoChange));

    let outcome = agent.poll_once(0).await.expect("poll 3");
    assert!(matches!(outcome, PollOutcome::NoChange));

    assert_eq!(responses_200.load(Ordering::SeqCst), 1);
    assert_eq!(responses_304.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn poll_once_relay_violation_200_for_rejected_etag() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let good_bytes = pvs_bytes("v1");
    let good_etag = etag_for_bytes(&good_bytes);
    let bad_bytes = Arc::new(vec![0u8; 100]);
    let bad_etag = etag_for_bytes(bad_bytes.as_ref());

    let app = {
        let bad_bytes_outer = Arc::clone(&bad_bytes);
        let bad_etag_outer = bad_etag.clone();
        Router::new().route(
            "/v1/config",
            get(move || {
                let bad_bytes_inner = Arc::clone(&bad_bytes_outer);
                let bad_etag_inner = bad_etag_outer.clone();
                async move {
                    use axum::response::Response;
                    let mut response =
                        Response::new(axum::body::Body::from(bad_bytes_inner.as_ref().clone()));
                    *response.status_mut() = StatusCode::OK;
                    response.headers_mut().insert(
                        axum::http::header::ETAG,
                        axum::http::HeaderValue::from_str(&format!("\"{}\"", bad_etag_inner))
                            .unwrap(),
                    );
                    response
                }
            }),
        )
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());
    agent
        .apply_update_for_tests(
            good_bytes,
            good_etag.clone(),
            Some(ConfigVersion(NonZeroU64::new(1).unwrap())),
        )
        .await
        .expect("apply baseline");
    agent.set_last_rejected_etag_with_ttl_for_tests(bad_etag.clone());

    let outcome = agent.poll_once(0).await.expect("poll 1");
    assert!(matches!(outcome, PollOutcome::NoChange));
}
