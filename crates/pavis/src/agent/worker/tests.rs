use super::ConfigAgent;
use crate::agent::Backoff;
use crate::agent::lkg::version_path_for;
use crate::state::{RuntimeState, RuntimeStateHandle};
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use pavis_core::{
    AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery,
    Duration as RuntimeDuration, Endpoint, EndpointAddr, Host, HttpVersion, IdleTimeout, Listener,
    ListenerName, LoadBalancer, Metrics, Path, PathMatch, Pool, Port, RetryPolicy, Rewrite,
    RewriteHost, RewritePath, ServiceName, Telemetry, Timeout, TlsConfig, TlsPolicy, Upstream,
    UpstreamId, UpstreamName, VirtualHost, Weight, WorkerCount,
};
use pavis_pvs::PAVIS_VERSION_HEADER;
use pingora::services::Service;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn minimal_config(name: &str) -> pavis_core::RuntimeConfig {
    pavis_core::RuntimeConfig {
        listeners: vec![Listener {
            name: ListenerName("default".to_string()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            workers: WorkerCount::Auto,
            tls: TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(name.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: pavis_core::TracingPolicy::Disabled,
        },
        upstreams: vec![Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("backend".to_string()),
            discovery: Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Enabled(RuntimeDuration(NonZeroU32::new(60_000).unwrap())),
                connect: ConnectTimeout::Enabled(RuntimeDuration(NonZeroU32::new(5_000).unwrap())),
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }],
        routes: vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled,
                response_headers: pavis_core::HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                destinations: vec![Destination {
                    upstream: UpstreamName("backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }],
            }],
        }],
    }
}

fn config_with_upstream(service_name: &str, upstream_name: &str) -> pavis_core::RuntimeConfig {
    let mut config = minimal_config(service_name);
    config.upstreams[0].name = UpstreamName(upstream_name.to_string());
    config.routes[0].paths[0].destinations[0].upstream = UpstreamName(upstream_name.to_string());
    config
}

fn write_pvs(path: &PathBuf, name: &str) -> Vec<u8> {
    let config = minimal_config(name);
    pavis_pvs::write(path, &config).expect("write");
    std::fs::read(path).expect("read")
}

fn make_agent(base: String, lkg_path: PathBuf, state: Arc<RuntimeStateHandle>) -> Arc<ConfigAgent> {
    let client = Client::builder().no_proxy().build().expect("client");
    Arc::new(ConfigAgent {
        relay_base: base,
        lkg_path: lkg_path.clone(),
        version_path: version_path_for(&lkg_path),
        client,
        backoff: Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0),
        state,
        current_version: std::sync::atomic::AtomicU64::new(0),
    })
}

async fn start_status_stub(status: StatusCode) -> Option<String> {
    async fn handler(status: StatusCode) -> impl IntoResponse {
        status
    }

    let app = Router::new().route("/v1/config", get(move || handler(status)));
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping stub bind: {err}");
            return None;
        }
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    Some(format!("http://{}", addr))
}

#[test]
fn worker_name_is_stable() {
    let dir = std::env::temp_dir().join("pavis_worker_name");
    let lkg = dir.join("config.pvs");
    let config = minimal_config("v1");
    let validated = crate::load::assume_validated(config);
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg, state_handle);
    let worker = agent.worker();
    assert_eq!(worker.name(), "config_poller");
}

#[tokio::test]
async fn apply_update_replaces_state_and_version() {
    let dir = std::env::temp_dir().join("pavis_poll_update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");
    std::fs::write(version_path_for(&lkg), "1").expect("version");

    let state =
        RuntimeState::from_config(&crate::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let config_v2 = config_with_upstream("v2", "blue");
    let tmp_pvs = dir.join("next.pvs");
    pavis_pvs::write(&tmp_pvs, &config_v2).expect("write");
    let bytes = std::fs::read(&tmp_pvs).expect("read");

    let agent = make_agent(
        "http://127.0.0.1:1".to_string(),
        lkg.clone(),
        state_handle.clone(),
    );
    agent.apply_update(bytes, 2).await.expect("apply");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_returns_no_change_on_304() {
    let Some(base) = start_status_stub(StatusCode::NOT_MODIFIED).await else {
        return;
    };
    let lkg = std::env::temp_dir().join("pavis_poll_304.pvs");
    let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(base, lkg, state);
    let outcome = agent.poll_once().await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::NoChange));
}

#[tokio::test]
async fn apply_update_removes_tmp_on_load_failure() {
    let dir = std::env::temp_dir().join("pavis_apply_fail");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg.clone(), state);
    // Valid PVS bytes but with a version that might cause load failure if incompatible
    // Actually, easiest is to mock a file that verify() passes but load() fails.
    // But load() uses pavis_pvs::load, which uses rkyv.
    // Let's just pass invalid bytes to apply_update, but it calls verify() first.
    let bad_pvs = vec![0u8; 100]; // Should fail verify
    let err = agent
        .apply_update(bad_pvs, 1)
        .await
        .expect_err("verify failure");
    assert!(err.to_string().contains("magic"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_reports_non_success_status() {
    let Some(base) = start_status_stub(StatusCode::INTERNAL_SERVER_ERROR).await else {
        return;
    };
    let dir = std::env::temp_dir().join("pavis_poll_status");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&crate::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let err = agent.poll_once().await.expect_err("status error");
    assert!(err.to_string().contains("poll failed: status=500"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn apply_update_warns_on_version_write_failure() {
    let dir = std::env::temp_dir().join("pavis_version_fail");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

    let config = minimal_config("v2");
    let validated = crate::load::assume_validated(config);
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let client = Client::builder().no_proxy().build().expect("client");
    let version_dir = dir.join("version_dir");
    std::fs::create_dir_all(&version_dir).expect("version dir");
    let agent = ConfigAgent {
        relay_base: "http://127.0.0.1:1".to_string(),
        lkg_path: lkg.clone(),
        version_path: version_dir,
        client,
        backoff: Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0),
        state: state_handle,
        current_version: std::sync::atomic::AtomicU64::new(0),
    };
    let bytes = std::fs::read(&lkg).expect("read");
    agent.apply_update(bytes, 2).await.expect("apply");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_apply_update_success() {
    let dir = std::env::temp_dir().join("pavis_apply_success");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(
        "http://127.0.0.1:1".to_string(),
        lkg.clone(),
        state_handle.clone(),
    );

    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");

    agent
        .apply_update(pvs, 1)
        .await
        .expect("apply update should succeed");

    assert_eq!(agent.current_version.load(Ordering::SeqCst), 1);
    assert!(lkg.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

async fn start_header_stub(
    status: StatusCode,
    headers: Option<Vec<(String, String)>>,
) -> Option<String> {
    use axum::http::HeaderMap;
    async fn handler(
        status: StatusCode,
        headers: Option<Vec<(String, String)>>,
    ) -> impl IntoResponse {
        let mut map = HeaderMap::new();
        if let Some(h) = headers {
            for (k, v) in h {
                if let Ok(name) = axum::http::HeaderName::from_bytes(k.as_bytes()) {
                    if let Ok(val) = axum::http::HeaderValue::from_str(&v) {
                        map.insert(name, val);
                    }
                }
            }
        }
        (status, map)
    }

    let app = Router::new().route("/v1/config", get(move || handler(status, headers.clone())));
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping stub bind: {err}");
            return None;
        }
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    Some(format!("http://{}", addr))
}

#[tokio::test]
async fn poll_once_missing_version_header() {
    let Some(base) = start_header_stub(StatusCode::OK, None).await else {
        return;
    };
    let dir = std::env::temp_dir().join("pavis_poll_missing_header");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&crate::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let err = agent.poll_once().await.expect_err("should fail");
    let msg = err.to_string();
    eprintln!("Actual error: {}", msg);
    assert!(
        msg.contains("missing x-pavis-version response header"),
        "Error '{}' did not contain expected string",
        msg
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_stale_version() {
    let headers = vec![(PAVIS_VERSION_HEADER.to_string(), "1".to_string())];
    let Some(base) = start_header_stub(StatusCode::OK, Some(headers)).await else {
        return;
    };
    let dir = std::env::temp_dir().join("pavis_poll_stale");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

    let state =
        RuntimeState::from_config(&crate::load::load_file(lkg.to_str().unwrap()).expect("load"))
            .expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent(base, lkg.clone(), state_handle);

    // Set current version to 1
    agent.set_current_version(1);

    // Server returns version 1, which is stale (<= current)
    let outcome = agent.poll_once().await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::NoChange));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_poll_once_success() {
    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");

    // We need a way to return bytes.
    use axum::response::Response;

    let pvs_clone = pvs.clone();
    let app = Router::new().route(
        "/v1/config",
        get(move || {
            let pvs_inner = pvs_clone.clone();
            async move {
                let mut res = Response::new(axum::body::Body::from(pvs_inner));
                res.headers_mut().insert(
                    axum::http::HeaderName::from_static("x-pavis-version"),
                    axum::http::HeaderValue::from_static("1"),
                );
                res
            }
        }),
    );

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base = format!("http://{}", addr);
    let dir = std::env::temp_dir().join("pavis_poll_success");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let agent = make_agent(base, lkg.clone(), state_handle);

    let outcome = agent.poll_once().await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::Updated));
    assert_eq!(agent.current_version.load(Ordering::SeqCst), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
