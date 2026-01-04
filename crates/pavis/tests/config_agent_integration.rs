use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use pavis::agent::{Backoff, ConfigAgent, PollOutcome, lkg_version, load_lkg_config};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis_core::{RuntimeConfig, TelemetryConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

#[derive(Clone)]
struct RelayStub {
    version: Arc<AtomicU64>,
    bytes: Arc<RwLock<Vec<u8>>>,
}

async fn relay_config(State(state): State<RelayStub>, headers: HeaderMap) -> impl IntoResponse {
    let client_version = headers
        .get("x-pavis-version")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let current = state.version.load(Ordering::SeqCst);
    if client_version == current {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let body = state.bytes.read().await.clone();
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        "x-pavis-version",
        HeaderValue::from_str(&current.to_string()).unwrap(),
    );
    response
}

fn minimal_config(label: &str) -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![pavis_core::Listener {
            name: "default".to_string(),
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: None,
            tls: None,
        }],
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: Some(label.to_string()),
            prometheus_addr: None,
            access_log: Default::default(),
            tracing: None,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

fn write_pvs(path: &PathBuf, label: &str) -> Vec<u8> {
    let config = minimal_config(label);
    pavis_pvs::write(path, &config).expect("write config");
    std::fs::read(path).expect("read config")
}

#[tokio::test]
async fn poller_updates_lkg_and_version() {
    let dir = std::env::temp_dir().join("pavis_poll_integration");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let lkg_path = dir.join("config.pvs");

    let bytes_v1 = write_pvs(&lkg_path, "v1");
    let version_path = lkg_path.with_extension("pvs.version");
    std::fs::write(&version_path, "1").expect("write version");

    let state =
        RuntimeState::from_config(&pavis::load::load_file(lkg_path.to_str().unwrap()).unwrap())
            .unwrap();
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let relay_state = RelayStub {
        version: Arc::new(AtomicU64::new(1)),
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
            Backoff::new(
                std::time::Duration::from_secs(1),
                std::time::Duration::from_secs(30),
                0,
            ),
        )
        .unwrap(),
    );
    agent.set_current_version(lkg_version(&lkg_path).unwrap());

    let outcome = agent.poll_once().await.unwrap();
    assert!(matches!(outcome, PollOutcome::NoChange));

    let tmp_path = dir.join("config_v2.pvs");
    let bytes_v2 = write_pvs(&tmp_path, "v2");
    let bytes_v2_expected = bytes_v2.clone();
    *relay_state.bytes.write().await = bytes_v2;
    relay_state.version.store(2, Ordering::SeqCst);

    let outcome = agent.poll_once().await.unwrap();
    assert!(matches!(outcome, PollOutcome::Updated));

    let on_disk = std::fs::read(&lkg_path).expect("read lkg");
    assert_eq!(on_disk, bytes_v2_expected);
    let lkg_version_value = lkg_version(&lkg_path).unwrap();
    assert_eq!(lkg_version_value, 2);
    let (validated, version) = load_lkg_config(&lkg_path).unwrap();
    assert_eq!(version, 2);
    assert_eq!(validated.telemetry.service_name.as_deref(), Some("v2"));

    let _ = std::fs::remove_dir_all(&dir);
}
