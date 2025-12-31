use anyhow::{Context, Result};
use pavis_core::{
    ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType, Route, RuntimeConfig,
    ServerConfig, TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
};
use pavis_e2e::support::relay::RelayOptions;
use pavis_e2e::support::{RelayEnv, find_binary, find_project_root, resolve_docker_service_ip};
use pavis_pvs;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

pub struct UpstreamEnv {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl UpstreamEnv {
    pub async fn new(body: &'static str) -> Result<Option<Self>> {
        if test_mode() == TestMode::Docker {
            return Ok(None);
        }
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping upstream bind: {err}");
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };
        let addr = listener.local_addr().context("upstream addr")?;
        let (tx, mut rx) = oneshot::channel::<()>();
        let body_bytes = body.as_bytes().to_vec();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    result = listener.accept() => {
                        let Ok((mut stream, _)) = result else { break };
                        let body = body_bytes.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 1024];
                            let _ = stream.read(&mut buf).await;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                            let _ = stream.write_all(&body).await;
                        });
                    }
                }
            }
        });

        Ok(Some(Self {
            addr,
            shutdown: Some(tx),
        }))
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for UpstreamEnv {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

pub struct PavisEnv {
    child: Option<Child>,
    container_id: Option<String>,
    compose_project: Option<String>,
    compose_file: Option<PathBuf>,
    compose_shared: bool,
    base_url: String,
    lkg_path: PathBuf,
    work_dir: PathBuf,
}

impl PavisEnv {
    pub fn new(config: &RuntimeConfig, host_port: u16, relay_url: &str) -> Result<Self> {
        let work_dir = unique_work_dir("pavis_integrated");
        std::fs::create_dir_all(&work_dir)?;
        let lkg_path = work_dir.join("config.pvs");
        pavis_pvs::write(&lkg_path, config).context("write lkg")?;
        let version_path = lkg_path.with_extension("pvs.version");
        std::fs::write(&version_path, "0").context("write version")?;

        let mut child = None;
        let mut container_id = None;
        let mut compose_project = None;
        let mut compose_file = None;
        let mut compose_shared = false;
        match test_mode() {
            TestMode::Binary => {
                let project_root = find_project_root()?;
                let pavis_bin = find_binary(&project_root, "pavis")?;
                let mut process = Command::new(&pavis_bin)
                    .arg("--config")
                    .arg(&lkg_path)
                    .arg("--relay-url")
                    .arg(relay_url)
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .context("spawn pavis")?;
                let _ = process.stdin.take();
                child = Some(process);
            }
            TestMode::Docker => {
                let image = resolve_image("PAVIS_IMAGE", "pavis:ci");
                let relay_url = std::env::var("PAVIS_RELAY_URL")
                    .ok()
                    .unwrap_or_else(|| relay_url_for_container(relay_url));
                let compose_override = std::env::var("PAVIS_COMPOSE_FILE").ok();
                if let Some(compose_path) = compose_override {
                    let compose_path = PathBuf::from(compose_path);
                    let project = std::env::var("PAVIS_COMPOSE_PROJECT")
                        .map(|value| {
                            compose_shared = true;
                            value
                        })
                        .unwrap_or_else(|_| {
                            format!(
                                "pavis-e2e-{}",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("time")
                                    .as_nanos()
                            )
                        });
                    let status = Command::new("docker")
                        .env("PAVIS_IMAGE", &image)
                        .env("PAVIS_PORT", host_port.to_string())
                        .env("PAVIS_WORK_DIR", work_dir.display().to_string())
                        .env("PAVIS_RELAY_URL", &relay_url)
                        .args([
                            "compose",
                            "-f",
                            compose_path.to_str().expect("valid path"),
                            "-p",
                            &project,
                            "up",
                            "-d",
                            "--no-deps",
                            "pavis",
                        ])
                        .status()
                        .context("spawn pavis container")?;
                    if !status.success() {
                        return Err(anyhow::anyhow!("Failed to start pavis container"));
                    }
                    compose_project = Some(project);
                    compose_file = Some(compose_path);
                } else {
                    let container_name = format!(
                        "pavis-e2e-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("time")
                            .as_nanos()
                    );
                    let output = Command::new("docker")
                        .args([
                            "run",
                            "-d",
                            "--rm",
                            "--name",
                            &container_name,
                            "-p",
                            &format!("{host_port}:8080"),
                            "-v",
                            &format!("{}:/pavis", work_dir.display()),
                            &image,
                            "--config",
                            "/pavis/config.pvs",
                            "--relay-url",
                            &relay_url,
                        ])
                        .output()
                        .context("spawn pavis container")?;
                    if !output.status.success() {
                        return Err(anyhow::anyhow!(
                            "Failed to start pavis container: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ));
                    }
                    container_id = Some(container_name);
                }
            }
        }

        Ok(Self {
            child,
            container_id,
            compose_project,
            compose_file,
            compose_shared,
            base_url: format!("http://127.0.0.1:{host_port}"),
            lkg_path,
            work_dir,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn version_path(&self) -> PathBuf {
        self.lkg_path.with_extension("pvs.version")
    }
}

impl Drop for PavisEnv {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(container) = self.container_id.take() {
            let _ = Command::new("docker")
                .args(["stop", &container])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let (Some(project), Some(compose_file)) =
            (self.compose_project.take(), self.compose_file.as_ref())
        {
            let action = if self.compose_shared { "stop" } else { "down" };
            let mut args = vec![
                "compose",
                "-f",
                compose_file.to_str().expect("valid path"),
                "-p",
                &project,
                action,
            ];
            if self.compose_shared {
                args.push("pavis");
            }
            let _ = Command::new("docker")
                .args(&args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

pub fn unique_work_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nanos}"))
}

pub fn pick_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").context("bind port")?;
    let port = listener.local_addr().context("read port")?.port();
    drop(listener);
    Ok(port)
}

pub fn runtime_config(
    listen_addr: SocketAddr,
    upstream_a: (&str, SocketAddr),
    upstream_b: (&str, SocketAddr),
    route_upstream: &str,
) -> RuntimeConfig {
    RuntimeConfig {
        server: ServerConfig {
            listen_addr,
            worker_threads: None,
            tls: None,
        },
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: Some("pavis-integrated".to_string()),
            prometheus_addr: None,
            access_log: Default::default(),
            tracing: None,
        },
        upstreams: vec![
            upstream(upstream_a.0, upstream_a.1),
            upstream(upstream_b.0, upstream_b.1),
        ],
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Prefix,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![WeightedDestination {
                    upstream: route_upstream.to_string(),
                    weight: 1,
                }],
            }],
        }],
    }
}

pub fn upstream(name: &str, addr: SocketAddr) -> Upstream {
    Upstream {
        name: name.to_string(),
        load_balancer: LoadBalancer::RoundRobin,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            ip: addr.ip(),
            port: addr.port(),
            weight: 1,
        }],
    }
}

pub struct UpstreamSet {
    pub a: SocketAddr,
    pub b: SocketAddr,
    _guards: Vec<UpstreamEnv>,
}

pub async fn upstreams() -> Result<Option<UpstreamSet>> {
    match test_mode() {
        TestMode::Binary => {
            let Some(a) = UpstreamEnv::new("A").await? else {
                return Ok(None);
            };
            let Some(b) = UpstreamEnv::new("B").await? else {
                return Ok(None);
            };
            Ok(Some(UpstreamSet {
                a: a.addr(),
                b: b.addr(),
                _guards: vec![a, b],
            }))
        }
        TestMode::Docker => {
            let project_root = find_project_root()?;
            let ip_a = resolve_docker_service_ip(&project_root, "backend-v1")?;
            let ip_b = resolve_docker_service_ip(&project_root, "backend-v2")?;
            let addr_a = SocketAddr::new(ip_a.parse::<IpAddr>()?, 8081);
            let addr_b = SocketAddr::new(ip_b.parse::<IpAddr>()?, 8082);
            Ok(Some(UpstreamSet {
                a: addr_a,
                b: addr_b,
                _guards: Vec::new(),
            }))
        }
    }
}

pub async fn publish(
    relay_base: &str,
    version: u64,
    config: &RuntimeConfig,
) -> Result<reqwest::Response> {
    let client = client()?;
    let bytes = pvs_bytes(config)?;
    let response = client
        .post(format!("{relay_base}/v1/publish"))
        .header("X-Pavis-Version", version.to_string())
        .body(bytes)
        .send()
        .await?;
    Ok(response)
}

pub fn pvs_bytes(config: &RuntimeConfig) -> Result<Vec<u8>> {
    let dir = unique_work_dir("pavis_integrated_pvs");
    std::fs::create_dir_all(&dir).context("create pvs dir")?;
    let path = dir.join("config.pvs");
    pavis_pvs::write(&path, config).context("write pvs")?;
    let bytes = std::fs::read(&path).context("read pvs")?;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(bytes)
}

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("client")?)
}

pub async fn wait_for_body(base_url: &str, expected: &str) -> Result<()> {
    let client = client()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() > deadline {
            return Err(anyhow::anyhow!("timeout waiting for response {expected}"));
        }
        if let Ok(resp) = client.get(format!("{base_url}/")).send().await {
            if let Ok(text) = resp.text().await {
                if text.contains(expected) {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub fn expected_body(label: &str) -> String {
    match test_mode() {
        TestMode::Binary => label.to_string(),
        TestMode::Docker => match label {
            "A" => "backend-v1".to_string(),
            "B" => "backend-v2".to_string(),
            _ => label.to_string(),
        },
    }
}

pub async fn wait_for_version(path: &Path, expected: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() > deadline {
            return Err(anyhow::anyhow!("timeout waiting for version {expected}"));
        }
        if let Ok(contents) = std::fs::read_to_string(path) {
            if contents.trim() == expected.to_string() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub struct PavisTarget {
    pub listen_addr: SocketAddr,
    pub host_port: u16,
}

pub fn pavis_target() -> Result<PavisTarget> {
    match test_mode() {
        TestMode::Binary => {
            let port = pick_port()?;
            Ok(PavisTarget {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
                host_port: port,
            })
        }
        TestMode::Docker => {
            let port = pick_port()?;
            Ok(PavisTarget {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                host_port: port,
            })
        }
    }
}

pub async fn relay_env() -> Result<RelayEnv> {
    RelayEnv::new(RelayOptions::default()).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestMode {
    Binary,
    Docker,
}

fn test_mode() -> TestMode {
    match std::env::var("TEST_MODE")
        .unwrap_or_else(|_| "binary".to_string())
        .as_str()
    {
        "docker" => TestMode::Docker,
        _ => TestMode::Binary,
    }
}

fn relay_url_for_container(url: &str) -> String {
    if url.contains("127.0.0.1") {
        url.replace("127.0.0.1", "host.docker.internal")
    } else if url.contains("localhost") {
        url.replace("localhost", "host.docker.internal")
    } else {
        url.to_string()
    }
}

fn resolve_image(env_key: &str, fallback: &str) -> String {
    let raw = std::env::var(env_key).unwrap_or_else(|_| fallback.to_string());
    if raw.contains(':') {
        return raw;
    }
    if docker_image_exists(&raw) {
        return raw;
    }
    let ci = format!("{raw}:ci");
    if docker_image_exists(&ci) {
        return ci;
    }
    let local = format!("{raw}:local");
    if docker_image_exists(&local) {
        return local;
    }
    raw
}

fn docker_image_exists(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
