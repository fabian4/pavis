use anyhow::{Context, Result};
use pavis_core::{RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::RelayEnv;
use reqwest::{Client, StatusCode};
use std::time::Duration;

fn minimal_config(label: &str) -> RuntimeConfig {
    RuntimeConfig {
        server: ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: None,
            tls: None,
        },
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

fn valid_pvs_bytes(label: &str) -> Vec<u8> {
    let config = minimal_config(label);
    let dir = std::env::temp_dir();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let path = dir.join(format!("pavis_relay_e2e_{label}_{pid}_{id}.pvs"));
    pavis_pvs::write(&path, &config).expect("write config");
    let bytes = std::fs::read(&path).expect("read config");
    let _ = std::fs::remove_file(&path);
    bytes
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build client")?)
}

#[tokio::test]
async fn relay_publish_validation_and_headers() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let payload = valid_pvs_bytes("seed");
    let response = client
        .post(format!("{base}/v1/publish"))
        .body(payload.clone())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(Vec::new())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body("bad")
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(payload.clone())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .get(format!("{base}/v1/config"))
        .header("X-Pavis-Version", "0")
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
    assert!(
        headers
            .get("x-pavis-checksum")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        headers
            .get("x-pavis-checksum-alg")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(
        headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        headers
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let body = response.bytes().await?;
    assert_eq!(body.as_ref(), payload.as_slice());

    Ok(())
}

#[tokio::test]
async fn relay_long_poll_updates() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let payload = valid_pvs_bytes("initial");
    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(payload)
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let wait = {
        let client = client.clone();
        let url = format!("{base}/v1/config?wait_ms=2000");
        tokio::spawn(async move { client.get(url).header("X-Pavis-Version", "1").send().await })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;

    let updated = valid_pvs_bytes("updated");
    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "2")
        .body(updated.clone())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = wait
        .await
        .context("long poll join")?
        .context("long poll request")?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );
    let body = response.bytes().await?;
    assert_eq!(body.as_ref(), updated.as_slice());

    Ok(())
}

#[tokio::test]
async fn relay_persists_lkg_across_restart() -> Result<()> {
    let mut env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let payload = valid_pvs_bytes("persist");
    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "3")
        .body(payload.clone())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let metadata = std::fs::metadata(env.lkg_path()).context("lkg metadata")?;
    assert!(metadata.len() > 0);

    env.restart().await?;

    let response = client.get(format!("{base}/ready")).send().await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .get(format!("{base}/v1/config"))
        .header("X-Pavis-Version", "1")
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok()),
        Some("0")
    );
    let body = response.bytes().await?;
    assert_eq!(body.as_ref(), payload.as_slice());

    Ok(())
}
