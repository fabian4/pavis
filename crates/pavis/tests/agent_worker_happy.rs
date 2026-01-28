mod common;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use common::*;
use pavis::agent::PollOutcome;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use std::sync::Arc;
use std::sync::Mutex;

#[test]
fn worker_name_is_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");
    let config = minimal_config("v1");
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg, state_handle);
    let worker = agent.worker();
    use pingora::services::Service;
    assert_eq!(worker.name(), "config_poller");
}

#[tokio::test]
async fn apply_update_replaces_state_and_caches_etag() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let config_v2 = config_with_upstream("v2", "blue");
    let tmp_pvs = dir.path().join("next.pvs");
    pavis_pvs::write(&tmp_pvs, &config_v2).expect("write");
    let bytes = std::fs::read(&tmp_pvs).expect("read");

    let agent = make_agent(
        "http://127.0.0.1:1".to_string(),
        lkg.clone(),
        state_handle.clone(),
    );
    let etag = etag_for_bytes(&bytes);
    agent
        .apply_update_for_tests(bytes, etag.clone(), None)
        .await
        .expect("apply");
    assert_eq!(agent.last_applied_etag_for_tests(), Some(etag));
}

#[tokio::test]
async fn poll_once_returns_no_change_on_304() {
    let app = Router::new().route("/v1/config", get(|| async { StatusCode::NOT_MODIFIED }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);

    let lkg = std::env::temp_dir().join("pavis_poll_304.pvs");
    let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(base, lkg, state);
    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, PollOutcome::NoChange));
}

#[tokio::test]
async fn test_apply_update_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(
        "http://127.0.0.1:1".to_string(),
        lkg.clone(),
        state_handle.clone(),
    );

    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");

    agent
        .apply_update_for_tests(pvs.clone(), etag_for_bytes(&pvs), None)
        .await
        .expect("apply update should succeed");

    assert_eq!(
        agent.last_applied_etag_for_tests(),
        Some(etag_for_bytes(&pvs))
    );
    assert!(lkg.exists());
}

#[tokio::test]
async fn poll_once_no_change_on_matching_etag() {
    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");
    let etag_val = etag_for_bytes(&pvs);
    let etag_val_clone = etag_val.to_string();
    let pvs_clone = pvs.clone();
    let app = Router::new().route(
        "/v1/config",
        get(move || {
            let etag_inner = etag_val_clone.clone();
            let body = pvs_clone.clone();
            async move {
                let mut res = axum::response::Response::new(axum::body::Body::from(body));
                *res.status_mut() = StatusCode::OK;
                res.headers_mut().insert(
                    axum::http::header::ETAG,
                    axum::http::HeaderValue::from_str(&format!("\"{}\"", etag_inner)).unwrap(),
                );
                res
            }
        }),
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

    agent.set_last_applied_etag_for_tests(Some(etag_val.to_string()));

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, PollOutcome::NoChange));
}

#[tokio::test]
async fn test_poll_once_success() {
    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");
    let etag = etag_for_bytes(&pvs);

    let pvs_clone = pvs.clone();
    let etag_clone = etag.clone();
    let app = Router::new().route(
        "/v1/config",
        get(move || {
            let pvs_inner = pvs_clone.clone();
            let etag_inner = etag_clone.clone();
            async move {
                let mut res = axum::response::Response::new(axum::body::Body::from(pvs_inner));
                res.headers_mut().insert(
                    axum::http::header::ETAG,
                    axum::http::HeaderValue::from_str(&format!("\"{}\"", etag_inner)).unwrap(),
                );
                res
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base = format!("http://{}", addr);
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, PollOutcome::Updated));
    assert_eq!(agent.last_applied_etag_for_tests(), Some(etag));
}

#[tokio::test]
async fn poll_once_skips_intermediate_versions_entirely() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg = dir.path().join("config.pvs");

    let base_state = minimal_config("bootstrap");
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(base_state) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let v1_bytes = pvs_bytes("v1");
    let v1_etag = etag_for_bytes(&v1_bytes);
    let v5_bytes = pvs_bytes("v5");
    let v5_etag = etag_for_bytes(&v5_bytes);

    let config_bytes = v5_bytes.clone();
    let config_etag = v5_etag.clone();

    let app = Router::new().route(
        "/v1/config",
        get(move || {
            let config_bytes_inner = config_bytes.clone();
            let config_etag_inner = config_etag.clone();
            async move {
                use axum::response::Response;
                let mut response =
                    Response::new(axum::body::Body::from(config_bytes_inner.clone()));
                *response.status_mut() = StatusCode::OK;
                let headers = response.headers_mut();
                headers.insert(
                    axum::http::header::ETAG,
                    axum::http::HeaderValue::from_str(&format!("\"{}\"", config_etag_inner))
                        .unwrap(),
                );
                headers.insert(
                    pavis_core::CONFIG_VERSION_HEADER,
                    axum::http::HeaderValue::from_static("5"),
                );
                headers.insert(
                    pavis_core::CONFIG_SIZE_HEADER,
                    axum::http::HeaderValue::from_str(&config_bytes_inner.len().to_string())
                        .unwrap(),
                );
                response
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());
    agent
        .apply_update_for_tests(v1_bytes, v1_etag, Some(1))
        .await
        .expect("apply v1");

    let observed = Arc::new(Mutex::new(Vec::new()));
    {
        let observed = Arc::clone(&observed);
        agent.on_update(move |config| {
            let mut guard = observed.lock().expect("lock");
            guard.push(config.telemetry.service_name.0.clone());
        });
    }

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, PollOutcome::Updated));

    let observed_list = observed.lock().expect("lock").clone();
    assert_eq!(observed_list, vec!["v5"]);
    assert_eq!(agent.last_applied_etag_for_tests(), Some(v5_etag));
}
