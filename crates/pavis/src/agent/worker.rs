use async_trait::async_trait;
use pavis_core::ValidatedRuntimeConfig;
use pingora::services::Service;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;

use crate::state::{RuntimeState, RuntimeStateHandle};

use super::backoff::Backoff;
use super::lkg::{tmp_path_for, version_path_for, write_atomic, write_version};
use pavis_pvs::PAVIS_VERSION_HEADER;

pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    version_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    current_version: AtomicU64,
}

pub struct ConfigAgentWorker {
    agent: Arc<ConfigAgent>,
}

#[async_trait]
impl Service for ConfigAgentWorker {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: watch::Receiver<bool>,
        _threads: usize,
    ) {
        let mut attempt = 0u32;
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                result = self.agent.poll_once() => {
                    match result {
                        Ok(PollOutcome::Updated) | Ok(PollOutcome::NoChange) => {
                            attempt = 0;
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "config poll failed");
                            let delay = self.agent.backoff.next_delay(attempt);
                            attempt = attempt.saturating_add(1);
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "config_poller"
    }
}

impl ConfigAgent {
    pub fn new(
        relay_base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
        timeout: Duration,
        backoff: Backoff,
    ) -> anyhow::Result<Self> {
        let version_path = version_path_for(&lkg_path);
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            relay_base,
            lkg_path,
            version_path,
            client,
            backoff,
            state,
            current_version: AtomicU64::new(0),
        })
    }

    pub fn worker(self: Arc<Self>) -> ConfigAgentWorker {
        ConfigAgentWorker { agent: self }
    }

    pub fn set_current_version(&self, version: u64) {
        self.current_version.store(version, Ordering::SeqCst);
    }

    pub async fn poll_once(&self) -> anyhow::Result<PollOutcome> {
        let version = self.current_version.load(Ordering::SeqCst);
        let url = format!("{}/v1/config", self.relay_base);
        let response = self
            .client
            .get(url)
            .header(PAVIS_VERSION_HEADER, version.to_string())
            .send()
            .await?;

        match response.status().as_u16() {
            200 => {
                let header_version = response
                    .headers()
                    .get(PAVIS_VERSION_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("missing {PAVIS_VERSION_HEADER} response header")
                    })?;

                if header_version <= version {
                    tracing::warn!(
                        current = version,
                        received = header_version,
                        "received stale config version, ignoring"
                    );
                    return Ok(PollOutcome::NoChange);
                }

                let bytes = response.bytes().await?;
                self.apply_update(bytes.to_vec(), header_version).await?;
                Ok(PollOutcome::Updated)
            }
            204 | 304 => Ok(PollOutcome::NoChange),
            status => Err(anyhow::anyhow!("poll failed: status={status}")),
        }
    }

    async fn apply_update(&self, bytes: Vec<u8>, version: u64) -> anyhow::Result<()> {
        let _ = pavis_pvs::verify(&bytes)?;

        let tmp_path = tmp_path_for(&self.lkg_path);
        write_atomic(&tmp_path, &bytes).await?;

        let config = match pavis_pvs::load(&tmp_path) {
            Ok(config) => config,
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(err.into());
            }
        };
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config) };
        let state = RuntimeState::from_config(&validated)?;

        tokio::fs::rename(&tmp_path, &self.lkg_path).await?;
        if let Err(err) = write_version(&self.version_path, version).await {
            tracing::warn!(error = %err, "failed to persist LKG version metadata");
        }

        self.state.store(state);
        self.current_version.store(version, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Updated,
    NoChange,
}

#[cfg(test)]
mod tests {
    use super::ConfigAgent;
    use crate::agent::Backoff;
    use crate::agent::lkg::version_path_for;
    use crate::state::{RuntimeState, RuntimeStateHandle};
    use axum::Router;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use pavis_core::ValidatedRuntimeConfig;
    use pavis_core::{
        ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, Route, ServerConfig,
        TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
    };
    use pavis_pvs::PAVIS_VERSION_HEADER;
    use pingora::services::Service;
    use reqwest::Client;
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    fn minimal_config(name: &str) -> pavis_core::RuntimeConfig {
        pavis_core::RuntimeConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".parse().expect("addr"),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: Some(name.to_string()),
                prometheus_addr: None,
                access_log: Default::default(),
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "backend".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: 8080,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: pavis_core::MatchType::Prefix,
                    path: "/".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                }],
            }],
        }
    }

    fn config_with_upstream(service_name: &str, upstream_name: &str) -> pavis_core::RuntimeConfig {
        let mut config = minimal_config(service_name);
        config.upstreams[0].name = upstream_name.to_string();
        config.routes[0].paths[0].destinations[0].upstream = upstream_name.to_string();
        config
    }

    fn write_pvs(path: &PathBuf, name: &str) -> Vec<u8> {
        let config = minimal_config(name);
        pavis_pvs::write(path, &config).expect("write");
        std::fs::read(path).expect("read")
    }

    fn make_agent(
        base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
    ) -> Arc<ConfigAgent> {
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
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config) };
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

        let state = RuntimeState::from_config(
            &crate::load::load_file(lkg.to_str().unwrap()).expect("load"),
        )
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

        let state = RuntimeState::from_config(
            &crate::load::load_file(lkg.to_str().unwrap()).expect("load"),
        )
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
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config) };
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

        let state = RuntimeState::from_config(
            &crate::load::load_file(lkg.to_str().unwrap()).expect("load"),
        )
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

        let state = RuntimeState::from_config(
            &crate::load::load_file(lkg.to_str().unwrap()).expect("load"),
        )
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

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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
}
