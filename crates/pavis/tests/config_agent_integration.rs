use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use pavis::agent::{ConfigAgent, PollOutcome};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis_core::{
    AccessLogPolicy, ListenerBuilder, ListenerName, Metrics, RuntimeConfig, RuntimeConfigBuilder,
    ServiceName, Telemetry, WorkerCount,
};
use pingora::services::Service;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify, RwLock};

#[derive(Clone)]
struct RelayStub {
    bytes: Arc<RwLock<Vec<u8>>>,
}

fn checksum_for_bytes(bytes: &[u8]) -> String {
    let digest = pavis_pvs::compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

async fn relay_config(State(state): State<RelayStub>, _headers: HeaderMap) -> impl IntoResponse {
    let body = state.bytes.read().await.clone();
    let checksum = checksum_for_bytes(&body);
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        "etag",
        HeaderValue::from_str(&format!("\"{checksum}\"")).unwrap(),
    );
    response
}

#[derive(Clone, Debug)]
struct RequestRecord {
    query: String,
    if_none_match: Option<String>,
}

#[derive(Clone)]
struct RelayRecorder {
    responses: Arc<RwLock<Vec<ResponseSpec>>>,
    requests: Arc<RwLock<Vec<RequestRecord>>>,
}

#[derive(Clone)]
enum ResponseSpec {
    Ok(Vec<u8>),
    NotModified,
    Gone,
    ServerError,
}

async fn relay_record(
    State(state): State<RelayRecorder>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let wait_ms = params.get("wait_ms").cloned().unwrap_or_default();
    let if_none_match = headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    state.requests.write().await.push(RequestRecord {
        query: format!("wait_ms={wait_ms}"),
        if_none_match,
    });

    let response = {
        let mut guard = state.responses.write().await;
        if guard.is_empty() {
            ResponseSpec::NotModified
        } else {
            guard.remove(0)
        }
    };
    match response {
        ResponseSpec::Ok(bytes) => {
            let checksum = checksum_for_bytes(&bytes);
            let mut response = bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(
                "etag",
                HeaderValue::from_str(&format!("\"{checksum}\"")).unwrap(),
            );
            response
        }
        ResponseSpec::NotModified => axum::http::StatusCode::NOT_MODIFIED.into_response(),
        ResponseSpec::Gone => axum::http::StatusCode::GONE.into_response(),
        ResponseSpec::ServerError => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Clone)]
struct ConcurrencyRecorder {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}

async fn relay_concurrency(State(state): State<ConcurrencyRecorder>) -> impl IntoResponse {
    let current = state.active.fetch_add(1, Ordering::SeqCst) + 1;
    state.max_active.fetch_max(current, Ordering::SeqCst);
    state.calls.fetch_add(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(200)).await;
    state.active.fetch_sub(1, Ordering::SeqCst);
    axum::http::StatusCode::NOT_MODIFIED.into_response()
}

#[derive(Clone)]
struct TimedRecorder {
    responses: Arc<Mutex<Vec<ResponseSpec>>>,
    times: Arc<Mutex<Vec<Instant>>>,
    notify: Arc<Notify>,
}

async fn relay_timed(State(state): State<TimedRecorder>) -> impl IntoResponse {
    let now = Instant::now();
    let mut times = state.times.lock().await;
    times.push(now);
    if times.len() >= 2 {
        state.notify.notify_one();
    }
    drop(times);

    let response = {
        let mut guard = state.responses.lock().await;
        if guard.is_empty() {
            ResponseSpec::NotModified
        } else {
            guard.remove(0)
        }
    };

    match response {
        ResponseSpec::Ok(bytes) => {
            let checksum = checksum_for_bytes(&bytes);
            let mut response = bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(
                "etag",
                HeaderValue::from_str(&format!("\"{checksum}\"")).unwrap(),
            );
            response
        }
        ResponseSpec::NotModified => axum::http::StatusCode::NOT_MODIFIED.into_response(),
        ResponseSpec::Gone => axum::http::StatusCode::GONE.into_response(),
        ResponseSpec::ServerError => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn minimal_config(label: &str) -> RuntimeConfig {
    let listener = ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address("127.0.0.1:0".parse().expect("addr"))
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(label.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config")
}

fn write_pvs(path: &PathBuf, label: &str) -> Vec<u8> {
    let config = minimal_config(label);
    pavis_pvs::write(path, &config).expect("write config");
    std::fs::read(path).expect("read config")
}

#[tokio::test]
async fn poller_updates_lkg_on_checksum_change() {
    let dir = std::env::temp_dir().join("pavis_poll_integration");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let lkg_path = dir.join("config.pvs");

    let bytes_v1 = write_pvs(&lkg_path, "v1");

    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg_path.to_str().unwrap()).unwrap())
            .unwrap();
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let relay_state = RelayStub {
        bytes: Arc::new(RwLock::new(bytes_v1)),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_config))
        .with_state(relay_state.clone());
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping relay integration test: {err}");
            return;
        }
        Err(err) => panic!("failed to bind relay stub: {err}"),
    };
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path.clone(),
            state_handle.clone(),
            std::time::Duration::from_secs(5),
        )
        .unwrap(),
    );

    let outcome = agent.poll_once(0).await.unwrap();
    assert!(matches!(outcome, PollOutcome::Updated));

    let outcome = agent.poll_once(0).await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));

    let tmp_path = dir.join("config_v2.pvs");
    let bytes_v2 = write_pvs(&tmp_path, "v2");
    let bytes_v2_expected = bytes_v2.clone();
    *relay_state.bytes.write().await = bytes_v2;

    let outcome = agent.poll_once(0).await.unwrap();
    assert!(matches!(outcome, PollOutcome::Updated));

    let on_disk = std::fs::read(&lkg_path).expect("read lkg");
    assert_eq!(on_disk, bytes_v2_expected);
    let (validated, _version) = pavis::agent::load_lkg_config(&lkg_path).unwrap();
    assert_eq!(validated.telemetry.service_name.0.as_str(), "v2");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poller_records_wait_ms_and_conditional_headers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg_path = dir.path().join("config.pvs");
    let bytes_v1 = write_pvs(&lkg_path, "v1");
    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg_path.to_str().unwrap()).unwrap())
            .unwrap();
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let recorder = RelayRecorder {
        responses: Arc::new(RwLock::new(vec![
            ResponseSpec::Ok(bytes_v1.clone()),
            ResponseSpec::NotModified,
        ])),
        requests: Arc::new(RwLock::new(Vec::new())),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_record))
        .with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path.clone(),
            state_handle.clone(),
            std::time::Duration::from_secs(5),
        )
        .unwrap(),
    );

    let outcome = agent.poll_once(30_000).await.unwrap();
    assert!(matches!(outcome, PollOutcome::Updated));

    let outcome = agent.poll_once(30_000).await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));

    let requests = recorder.requests.read().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].query, "wait_ms=30000");
    assert!(requests[0].if_none_match.is_none());
    assert_eq!(requests[1].query, "wait_ms=30000");
    assert!(requests[1].if_none_match.is_some());
}

#[tokio::test]
async fn poller_clears_conditional_state_on_resync() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg_path = dir.path().join("config.pvs");
    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));

    let recorder = RelayRecorder {
        responses: Arc::new(RwLock::new(vec![
            ResponseSpec::Gone,
            ResponseSpec::NotModified,
        ])),
        requests: Arc::new(RwLock::new(Vec::new())),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_record))
        .with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path.clone(),
            state_handle.clone(),
            std::time::Duration::from_secs(5),
        )
        .unwrap(),
    );

    agent.set_last_applied_etag_for_tests(Some(
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
    ));

    let outcome = agent.poll_once(30_000).await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));
    assert!(agent.last_applied_etag_for_tests().is_none());

    let outcome = agent.poll_once(30_000).await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));

    let requests = recorder.requests.read().await;
    assert_eq!(requests.len(), 2);
    assert!(requests[0].if_none_match.is_some());
    assert!(requests[1].if_none_match.is_none());
}

#[tokio::test]
async fn poller_treats_5xx_as_transient_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg_path = dir.path().join("config.pvs");
    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));

    let recorder = RelayRecorder {
        responses: Arc::new(RwLock::new(vec![ResponseSpec::ServerError])),
        requests: Arc::new(RwLock::new(Vec::new())),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_record))
        .with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path.clone(),
            state_handle.clone(),
            std::time::Duration::from_secs(5),
        )
        .unwrap(),
    );

    let outcome = agent.poll_once(30_000).await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));
}

#[tokio::test]
async fn worker_enforces_single_inflight_fetch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg_path = dir.path().join("config.pvs");
    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));

    let recorder = ConcurrencyRecorder {
        active: Arc::new(AtomicUsize::new(0)),
        max_active: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_concurrency))
        .with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path,
            state_handle,
            Duration::from_secs(5),
        )
        .unwrap(),
    );

    let mut worker = agent.worker();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let worker_task = tokio::spawn(async move {
        worker.start_service(None, shutdown_rx, 1).await;
    });

    tokio::time::sleep(Duration::from_millis(650)).await;
    let _ = shutdown_tx.send(true);
    let _ = worker_task.await;

    assert!(recorder.calls.load(Ordering::SeqCst) >= 1);
    assert!(recorder.max_active.load(Ordering::SeqCst) <= 1);
}

#[tokio::test]
async fn worker_applies_backoff_after_5xx() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lkg_path = dir.path().join("config.pvs");
    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));

    let recorder = TimedRecorder {
        responses: Arc::new(Mutex::new(vec![
            ResponseSpec::ServerError,
            ResponseSpec::NotModified,
        ])),
        times: Arc::new(Mutex::new(Vec::new())),
        notify: Arc::new(Notify::new()),
    };

    let app = Router::new()
        .route("/v1/config", get(relay_timed))
        .with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let agent = Arc::new(
        ConfigAgent::new(
            format!("http://{}", addr),
            lkg_path,
            state_handle,
            Duration::from_secs(5),
        )
        .unwrap(),
    );

    let mut worker = agent.worker();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let worker_task = tokio::spawn(async move {
        worker.start_service(None, shutdown_rx, 1).await;
    });

    tokio::time::timeout(Duration::from_secs(5), recorder.notify.notified())
        .await
        .expect("timed out waiting for requests");
    let _ = shutdown_tx.send(true);
    let _ = worker_task.await;

    let times = recorder.times.lock().await;
    assert!(times.len() >= 2);
    let delta = times[1].duration_since(times[0]);
    assert!(delta >= Duration::from_millis(200));
    assert!(delta < Duration::from_secs(2));
}
