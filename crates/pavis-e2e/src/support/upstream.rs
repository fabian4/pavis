use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use super::pavis::resolve_docker_service_ip;
use crate::support::pavis::find_project_root;

pub struct UpstreamEnv {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl UpstreamEnv {
    pub async fn new(body: &'static str) -> Result<Option<Self>> {
        if is_docker() {
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

pub struct UpstreamSet {
    pub a: SocketAddr,
    pub b: SocketAddr,
    _guards: Vec<UpstreamEnv>,
}

impl UpstreamSet {
    pub async fn new() -> Result<Self> {
        if !is_docker() {
            let Some(a) = UpstreamEnv::new("A").await? else {
                return Err(anyhow::anyhow!("Failed to create upstream A"));
            };
            let Some(b) = UpstreamEnv::new("B").await? else {
                return Err(anyhow::anyhow!("Failed to create upstream B"));
            };
            Ok(Self {
                a: a.addr(),
                b: b.addr(),
                _guards: vec![a, b],
            })
        } else {
            let project_root = find_project_root()?;
            let ip_a = resolve_docker_service_ip(&project_root, "backend-v1")?;
            let ip_b = resolve_docker_service_ip(&project_root, "backend-v2")?;
            let addr_a = SocketAddr::new(ip_a.parse::<IpAddr>()?, 8081);
            let addr_b = SocketAddr::new(ip_b.parse::<IpAddr>()?, 8082);
            Ok(Self {
                a: addr_a,
                b: addr_b,
                _guards: Vec::new(),
            })
        }
    }
}

fn is_docker() -> bool {
    std::env::var("TEST_MODE")
        .map(|v| v == "docker")
        .unwrap_or(false)
}

pub fn expected_body(label: &str) -> String {
    if is_docker() {
        match label {
            "A" => "backend-v1".to_string(),
            "B" => "backend-v2".to_string(),
            _ => label.to_string(),
        }
    } else {
        label.to_string()
    }
}
