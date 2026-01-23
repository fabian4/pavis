use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use axum_server::{Handle, tls_rustls::RustlsConfig};

use crate::common::cli::UpstreamArgs;
use crate::common::shutdown;
use crate::upstream::routes::{SharedState, TransportMeta, router};
use crate::upstream::tls::{self, TlsConfigPaths};

pub async fn run(args: UpstreamArgs) -> Result<()> {
    // Load config file to extract delay_ms and failure_sequence if provided
    let (delay_ms, failure_sequence) = if let Some(config_path) = &args.config {
        let content = std::fs::read_to_string(config_path)?;
        let config: crate::common::cli::UpstreamConfigFile = serde_json::from_str(&content)?;
        (config.delay_ms, config.failure_sequence)
    } else {
        (None, None)
    };

    let shared_state =
        SharedState::with_config(args.instance_id.clone(), delay_ms, failure_sequence);

    let http_addr = SocketAddr::new(args.bind_addr, args.http_port);
    let https_addr = SocketAddr::new(args.bind_addr, args.https_port);

    let tls_config = if let (Some(cert), Some(key)) = (args.cert_path, args.key_path) {
        let paths = TlsConfigPaths {
            cert_path: cert,
            key_path: key,
        };
        Some(tls::rustls_config(&paths).await?)
    } else {
        None
    };

    let http_router = router(shared_state.clone(), TransportMeta::http());
    let https_router = if tls_config.is_some() {
        Some(router(shared_state, TransportMeta::https()))
    } else {
        None
    };

    let http_handle = Handle::new();
    let https_handle = Handle::new();

    let mut http_task = tokio::spawn(run_http(http_addr, http_router, http_handle.clone()));

    let mut https_task = if let (Some(cfg), Some(router)) = (tls_config, https_router) {
        Some(tokio::spawn(run_https(
            https_addr,
            router,
            cfg,
            https_handle.clone(),
        )))
    } else {
        None
    };

    let shutdown_signal = shutdown::wait();

    tokio::select! {
        res = &mut http_task => {
            res??;
        }
        _ = async {
            if let Some(task) = &mut https_task {
                if let Err(e) = task.await {
                    tracing::error!("HTTPS task failed: {}", e);
                }
            } else {
                std::future::pending::<()>().await;
            }
        } => {}
        _ = shutdown_signal => {
            tracing::info!("shutdown signal received");
        }
    }

    // Graceful shutdown
    http_handle.shutdown();
    https_handle.shutdown();

    if !http_task.is_finished() {
        let _ = http_task.await;
    }
    if let Some(task) = https_task
        && !task.is_finished()
    {
        let _ = task.await;
    }

    Ok(())
}

async fn run_http(addr: SocketAddr, router: Router, handle: Handle) -> Result<()> {
    tracing::info!(%addr, "HTTP listener ready");
    axum_server::bind(addr)
        .handle(handle)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}

async fn run_https(
    addr: SocketAddr,
    router: Router,
    config: RustlsConfig,
    handle: Handle,
) -> Result<()> {
    tracing::info!(%addr, "HTTPS listener ready");
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}
