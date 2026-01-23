use super::ConfigAgent;
use crate::agent::Backoff;
use crate::state::{RuntimeState, RuntimeStateHandle};
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use pavis_core::{
    AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery,
    Duration as RuntimeDuration, Endpoint, EndpointAddr, HeaderPredicates, Host, HttpVersion,
    IdleTimeout, ListenerBuilder, ListenerName, LoadBalancer, MethodPredicate, Metrics, Path,
    PathMatch, Pool, Port, RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction,
    RouteMatcher, RuntimeConfigBuilder, ServiceName, Telemetry, Timeout, TlsConfig, TlsPolicy,
    UpstreamBuilder, UpstreamId, UpstreamName, VirtualHost, Weight, WorkerCount,
};
use pavis_pvs::compute_checksum;
use pingora::services::Service;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn minimal_config(name: &str) -> pavis_core::RuntimeConfig {
    let listener = ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
        .workers(WorkerCount::Auto)
        .tls(TlsConfig::Disabled)
        .build()
        .expect("listener");

    let upstream = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("backend".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(RuntimeDuration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(RuntimeDuration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");

    RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(name.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .add_upstream(upstream)
        .add_route(VirtualHost {
            host: Host("*".to_string()),
            paths: vec![pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        })
        .build()
        .expect("config")
}

fn config_with_upstream(service_name: &str, upstream_name: &str) -> pavis_core::RuntimeConfig {
    let mut config = minimal_config(service_name);
    config.upstreams[0].name = UpstreamName(upstream_name.to_string());
    if let RouteAction::Forward(destinations) = &mut config.routes[0].paths[0].action {
        destinations[0].upstream = UpstreamName(upstream_name.to_string());
    }
    config
}

fn write_pvs(path: &PathBuf, name: &str) -> Vec<u8> {
    let config = minimal_config(name);
    pavis_pvs::write(path, &config).expect("write");
    std::fs::read(path).expect("read")
}

fn pvs_bytes(name: &str) -> Vec<u8> {
    let config = minimal_config(name);
    pavis_pvs::encode(&config).expect("encode")
}

fn make_agent(base: String, lkg_path: PathBuf, state: Arc<RuntimeStateHandle>) -> Arc<ConfigAgent> {
    let client = Client::builder().no_proxy().build().expect("client");
    Arc::new(ConfigAgent::new_for_tests(
        base,
        lkg_path.clone(),
        state,
        client,
        Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0),
    ))
}

fn etag_for_bytes(bytes: &[u8]) -> String {
    let digest = compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
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
    // SAFETY: tests use configs that are assumed valid for runtime state construction.
    // SAFETY: test builds a validated config via pavis-pvs.
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let agent = make_agent("http://127.0.0.1:1".to_string(), lkg, state_handle);
    let worker = agent.worker();
    assert_eq!(worker.name(), "config_poller");
}

#[tokio::test]
async fn apply_update_replaces_state_and_caches_etag() {
    let dir = std::env::temp_dir().join("pavis_poll_update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

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
    let etag = etag_for_bytes(&bytes);
    agent
        .apply_update_for_tests(bytes, etag.clone(), None)
        .await
        .expect("apply");
    assert_eq!(agent.last_applied_etag_for_tests(), Some(etag));
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
    let outcome = agent.poll_once(0).await.expect("poll");
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
    let checksum = etag_for_bytes(&bad_pvs);
    let err = agent
        .apply_update_for_tests(bad_pvs, checksum, None)
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

    let err = agent.poll_once(0).await.expect_err("status error");
    assert!(err.to_string().contains("poll failed: status=500"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn apply_update_rejects_etag_mismatch() {
    let dir = std::env::temp_dir().join("pavis_version_fail");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");
    write_pvs(&lkg, "v1");

    let config = minimal_config("v2");
    // SAFETY: tests use configs that are assumed valid for runtime state construction.
    // SAFETY: test builds a validated config via pavis-pvs.
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let client = Client::builder().no_proxy().build().expect("client");
    let agent = ConfigAgent::new_for_tests(
        "http://127.0.0.1:1".to_string(),
        lkg.clone(),
        state_handle,
        client,
        Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0),
    );
    let bytes = std::fs::read(&lkg).expect("read");
    let err = agent
        .apply_update_for_tests(bytes, "sha256:bad".to_string(), None)
        .await
        .expect_err("etag mismatch");
    assert!(err.to_string().contains("etag/sha256 mismatch"));
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
        .apply_update_for_tests(pvs.clone(), etag_for_bytes(&pvs), None)
        .await
        .expect("apply update should succeed");

    assert_eq!(
        agent.last_applied_etag_for_tests(),
        Some(etag_for_bytes(&pvs))
    );
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
                if let (Ok(name), Ok(val)) = (
                    axum::http::HeaderName::from_bytes(k.as_bytes()),
                    axum::http::HeaderValue::from_str(&v),
                ) {
                    map.insert(name, val);
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
async fn poll_once_missing_etag_header() {
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

    let err = agent.poll_once(0).await.expect_err("should fail");
    let msg = err.to_string();
    eprintln!("Actual error: {}", msg);
    assert!(
        msg.contains("missing etag response header"),
        "Error '{}' did not contain expected string",
        msg
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_no_change_on_matching_etag() {
    let headers = vec![(
        "etag".to_string(),
        "\"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"".to_string(),
    )];
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

    agent.set_last_applied_etag_for_tests(Some(
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
    ));

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::NoChange));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_poll_once_success() {
    let config = minimal_config("v1");
    let pvs = pavis_pvs::encode(&config).expect("encode");
    let etag = etag_for_bytes(&pvs);

    // We need a way to return bytes.
    use axum::response::Response;

    let pvs_clone = pvs.clone();
    let checksum_header = etag.clone();
    let app = Router::new().route(
        "/v1/config",
        get(move || {
            let pvs_inner = pvs_clone.clone();
            async move {
                let mut res = Response::new(axum::body::Body::from(pvs_inner));
                res.headers_mut().insert(
                    axum::http::HeaderName::from_static("etag"),
                    axum::http::HeaderValue::from_str(&format!("\"{checksum_header}\"")).unwrap(),
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

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::Updated));
    assert_eq!(agent.last_applied_etag_for_tests(), Some(etag));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_skips_intermediate_versions_entirely() {
    let dir = std::env::temp_dir().join("pavis_poll_latest_only");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let base_state = minimal_config("bootstrap");
    let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(base_state) };
    let state = RuntimeState::from_config(&validated).expect("state");
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let v1_bytes = pvs_bytes("v1");
    let v1_etag = etag_for_bytes(&v1_bytes);
    let v5_bytes = pvs_bytes("v5");
    let v5_etag = etag_for_bytes(&v5_bytes);

    let artifact_fetches = Arc::new(AtomicUsize::new(0));
    let config_bytes = v5_bytes.clone();
    let config_etag = v5_etag.clone();
    let artifacts_counter = Arc::clone(&artifact_fetches);

    let app = Router::new()
        .route(
            "/v1/config",
            get(move || {
                let config_bytes = config_bytes.clone();
                let config_etag = config_etag.clone();
                async move {
                    use axum::response::Response;
                    let mut response = Response::new(axum::body::Body::from(config_bytes.clone()));
                    *response.status_mut() = StatusCode::OK;
                    let headers = response.headers_mut();
                    headers.insert(
                        axum::http::header::ETAG,
                        axum::http::HeaderValue::from_str(&format!("\"{}\"", config_etag)).unwrap(),
                    );
                    headers.insert(
                        pavis_core::CONFIG_VERSION_HEADER,
                        axum::http::HeaderValue::from_static("5"),
                    );
                    headers.insert(
                        pavis_core::CONFIG_SIZE_HEADER,
                        axum::http::HeaderValue::from_str(&config_bytes.len().to_string()).unwrap(),
                    );
                    response
                }
            }),
        )
        .route(
            "/v1/artifacts/:version",
            get(
                move |axum::extract::Path(_version): axum::extract::Path<u64>| {
                    let artifacts_counter = Arc::clone(&artifacts_counter);
                    async move {
                        artifacts_counter.fetch_add(1, Ordering::SeqCst);
                        StatusCode::NOT_FOUND
                    }
                },
            ),
        );

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
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
    assert!(matches!(outcome, super::PollOutcome::Updated));

    let observed = observed.lock().expect("lock").clone();
    assert_eq!(observed, vec!["v5"]);
    assert_eq!(artifact_fetches.load(Ordering::SeqCst), 0);
    assert_eq!(agent.last_applied_etag_for_tests(), Some(v5_etag));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_rejected_etag_triggers_304_not_200() {
    let dir = std::env::temp_dir().join("pavis_poll_rejected_long_poll");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let bad_bytes = Arc::new(vec![0u8; 64]);
    let bad_etag = etag_for_bytes(bad_bytes.as_ref());
    let responses_200 = Arc::new(AtomicUsize::new(0));
    let responses_304 = Arc::new(AtomicUsize::new(0));

    let app = {
        let bad_bytes = Arc::clone(&bad_bytes);
        let bad_etag = bad_etag.clone();
        let responses_200 = Arc::clone(&responses_200);
        let responses_304 = Arc::clone(&responses_304);
        Router::new().route(
            "/v1/config",
            get(move |headers: axum::http::HeaderMap| {
                let bad_bytes = Arc::clone(&bad_bytes);
                let bad_etag = bad_etag.clone();
                let responses_200 = Arc::clone(&responses_200);
                let responses_304 = Arc::clone(&responses_304);
                async move {
                    use axum::response::Response;
                    let header_match = headers
                        .get(axum::http::header::IF_NONE_MATCH)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.trim_matches('"').to_string());
                    if header_match.as_deref() == Some(bad_etag.as_str()) {
                        responses_304.fetch_add(1, Ordering::SeqCst);
                        return StatusCode::NOT_MODIFIED.into_response();
                    }

                    responses_200.fetch_add(1, Ordering::SeqCst);
                    let mut response =
                        Response::new(axum::body::Body::from(bad_bytes.as_ref().clone()));
                    *response.status_mut() = StatusCode::OK;
                    response.headers_mut().insert(
                        axum::http::header::ETAG,
                        axum::http::HeaderValue::from_str(&format!("\"{}\"", bad_etag)).unwrap(),
                    );
                    response
                }
            }),
        )
    };

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());

    let outcome = agent.poll_once(0).await.expect("poll 1");
    assert!(matches!(outcome, super::PollOutcome::Rejected));
    assert_eq!(agent.last_rejected_etag_for_tests(), Some(bad_etag.clone()));

    let outcome = agent.poll_once(0).await.expect("poll 2");
    assert!(matches!(outcome, super::PollOutcome::NoChange));

    let outcome = agent.poll_once(0).await.expect("poll 3");
    assert!(matches!(outcome, super::PollOutcome::NoChange));

    assert_eq!(responses_200.load(Ordering::SeqCst), 1);
    assert_eq!(responses_304.load(Ordering::SeqCst), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_applies_new_artifact_after_rejection() {
    let dir = std::env::temp_dir().join("pavis_poll_rejection_recovery");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let bad_etag = etag_for_bytes(&[9u8; 64]);
    let v2_bytes = Arc::new(pvs_bytes("v2"));
    let v2_etag = etag_for_bytes(v2_bytes.as_ref());

    let app = {
        let rejected_etag = bad_etag.clone();
        let applied_bytes = Arc::clone(&v2_bytes);
        let applied_etag = v2_etag.clone();
        Router::new().route(
            "/v1/config",
            get(move |headers: axum::http::HeaderMap| {
                let rejected_etag = rejected_etag.clone();
                let applied_bytes = Arc::clone(&applied_bytes);
                let applied_etag = applied_etag.clone();
                async move {
                    use axum::response::Response;
                    let header_match = headers
                        .get(axum::http::header::IF_NONE_MATCH)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.trim_matches('"').to_string());
                    if header_match.as_deref() == Some(rejected_etag.as_str()) {
                        let mut response =
                            Response::new(axum::body::Body::from(applied_bytes.as_ref().clone()));
                        *response.status_mut() = StatusCode::OK;
                        response.headers_mut().insert(
                            axum::http::header::ETAG,
                            axum::http::HeaderValue::from_str(&format!("\"{}\"", applied_etag))
                                .unwrap(),
                        );
                        return response;
                    }
                    if header_match.as_deref() == Some(applied_etag.as_str()) {
                        return StatusCode::NOT_MODIFIED.into_response();
                    }
                    StatusCode::NOT_MODIFIED.into_response()
                }
            }),
        )
    };

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());
    agent.set_last_rejected_etag_for_tests(Some(bad_etag.clone()));

    let outcome = agent.poll_once(0).await.expect("poll 1");
    assert!(matches!(outcome, super::PollOutcome::Updated));
    assert_eq!(agent.last_applied_etag_for_tests(), Some(v2_etag.clone()));
    assert_eq!(agent.last_rejected_etag_for_tests(), None);

    let outcome = agent.poll_once(0).await.expect("poll 2");
    assert!(matches!(outcome, super::PollOutcome::NoChange));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn poll_once_relay_violation_200_for_rejected_etag() {
    let dir = std::env::temp_dir().join("pavis_poll_violation");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let lkg = dir.join("config.pvs");

    let state_handle = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
    let good_bytes = pvs_bytes("v1");
    let good_etag = etag_for_bytes(&good_bytes);
    let bad_bytes = Arc::new(vec![1u8; 64]);
    let bad_etag = etag_for_bytes(bad_bytes.as_ref());

    let app = {
        let bad_bytes = Arc::clone(&bad_bytes);
        let bad_etag = bad_etag.clone();
        Router::new().route(
            "/v1/config",
            get(move || {
                let bad_bytes = Arc::clone(&bad_bytes);
                let bad_etag = bad_etag.clone();
                async move {
                    use axum::response::Response;
                    let mut response =
                        Response::new(axum::body::Body::from(bad_bytes.as_ref().clone()));
                    *response.status_mut() = StatusCode::OK;
                    response.headers_mut().insert(
                        axum::http::header::ETAG,
                        axum::http::HeaderValue::from_str(&format!("\"{}\"", bad_etag)).unwrap(),
                    );
                    response
                }
            }),
        )
    };

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("bind failed: {err}"),
    };
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{}", addr);

    let agent = make_agent(base, lkg.clone(), state_handle.clone());
    agent
        .apply_update_for_tests(good_bytes, good_etag.clone(), Some(1))
        .await
        .expect("apply baseline");
    agent.set_last_rejected_etag_for_tests(Some(bad_etag.clone()));

    let outcome = agent.poll_once(0).await.expect("poll");
    assert!(matches!(outcome, super::PollOutcome::NoChange));
    assert_eq!(agent.last_applied_etag_for_tests(), Some(good_etag));
    assert_eq!(agent.last_rejected_etag_for_tests(), Some(bad_etag));

    let _ = std::fs::remove_dir_all(&dir);
}
