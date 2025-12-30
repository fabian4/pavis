use anyhow::{Context, Result};
use pavis::agent::{Backoff, ConfigAgent, PollOutcome, lkg_version};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis_core::ValidatedRuntimeConfig;
use pavis_e2e::support::{RelayEnv, build_pvs_bytes};
use pavis_pvs;
use pavis_pvs::PAVIS_VERSION_HEADER;
use reqwest::{Client, StatusCode};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build client")?)
}

fn relay_lkg_dir() -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("pavis_agent_e2e_{pid}_{id}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn minimal_validated(label: &str) -> ValidatedRuntimeConfig {
    let config = pavis_core::RuntimeConfig {
        server: pavis_core::ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: None,
            tls: None,
        },
        telemetry: pavis_core::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: Some(label.to_string()),
            prometheus_addr: None,
            access_log: Default::default(),
            tracing: None,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    };
    unsafe { ValidatedRuntimeConfig::from_trusted(config) }
}

#[tokio::test]
async fn relay_publish_validation_and_headers() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .body(Vec::new())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = client
        .post(format!("{base}/v1/publish"))
        .body(build_pvs_bytes("seed"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(b"bad".to_vec())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(build_pvs_bytes("seed"))
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
    assert!(pavis_pvs::inspect(&body).is_ok());

    Ok(())
}

#[tokio::test]
async fn config_agent_polls_relay_with_header_contract() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header(PAVIS_VERSION_HEADER, "1")
        .body(build_pvs_bytes("agent-seed"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let work_dir = relay_lkg_dir();
    let lkg_path = work_dir.join("config.pvs");

    let state = RuntimeState::from_config(&minimal_validated("agent"))?;
    let state_handle = Arc::new(RuntimeStateHandle::new(state));

    let agent = Arc::new(ConfigAgent::new(
        base,
        lkg_path.clone(),
        state_handle,
        Duration::from_secs(5),
        Backoff::new(Duration::from_secs(1), Duration::from_secs(5), 0),
    )?);
    agent.set_current_version(0);

    let outcome = agent.poll_once().await?;
    assert!(matches!(outcome, PollOutcome::Updated));
    assert_eq!(lkg_version(&lkg_path)?, 1);

    let _ = std::fs::remove_dir_all(&work_dir);
    Ok(())
}

#[tokio::test]
async fn relay_long_poll_updates() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(build_pvs_bytes("initial"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let wait = {
        let client = client.clone();
        let url = format!("{base}/v1/config?wait_ms=2000");
        tokio::spawn(async move { client.get(url).header("X-Pavis-Version", "1").send().await })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "2")
        .body(build_pvs_bytes("updated"))
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
    assert!(pavis_pvs::inspect(&body).is_ok());

    Ok(())
}

#[tokio::test]
async fn relay_long_poll_times_out_with_no_content() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(build_pvs_bytes("initial"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .get(format!("{base}/v1/config?wait_ms=1"))
        .header("X-Pavis-Version", "1")
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    Ok(())
}

#[tokio::test]
async fn relay_reports_status_and_metrics() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(build_pvs_bytes("seed"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client.get(format!("{base}/v1/status")).send().await?;
    assert_eq!(response.status(), StatusCode::OK);
    let status = response.text().await?;
    assert!(status.contains("version="));
    assert!(status.contains("checksum="));

    let response = client.get(format!("{base}/v1/metrics")).send().await?;
    assert_eq!(response.status(), StatusCode::OK);
    let metrics = response.text().await?;
    assert!(metrics.contains("pavis_relay_publish_total"));
    assert!(metrics.contains("pavis_relay_publish_fail_total"));
    assert!(metrics.contains("pavis_relay_longpoll_wait_total"));

    Ok(())
}

#[tokio::test]
async fn relay_missing_artifact_returns_404() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .get(format!("{base}/v1/artifacts/999"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn relay_publish_fails_when_lkg_is_read_only() -> Result<()> {
    let env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "1")
        .body(build_pvs_bytes("seed"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let lkg_path = env.lkg_path().to_path_buf();
    let before = fs::read(&lkg_path)?;

    // Replace file with directory to block writing
    // This works regardless of file ownership as long as we own the parent directory
    fs::remove_file(&lkg_path)?;
    fs::create_dir(&lkg_path)?;

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "2")
        .body(build_pvs_bytes("blocked"))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Restore original file
    fs::remove_dir(&lkg_path)?;
    fs::write(&lkg_path, &before)?;

    let after = fs::read(&lkg_path)?;
    assert_eq!(before, after);

    Ok(())
}

#[tokio::test]
async fn relay_persists_lkg_across_restart() -> Result<()> {
    let mut env = RelayEnv::new().await?;
    let client = client()?;
    let base = env.base_url().to_string();

    let response = client
        .post(format!("{base}/v1/publish"))
        .header("X-Pavis-Version", "3")
        .body(build_pvs_bytes("persist"))
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
    assert!(pavis_pvs::inspect(&body).is_ok());

    Ok(())
}
