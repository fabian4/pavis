mod config;
mod routes;
mod tls;
mod types;

use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use axum_server::{Handle, tls_rustls::RustlsConfig};
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::routes::{SharedState, TransportMeta, router};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let shared_state = SharedState::new(config.instance_id().to_string());
    let http_addr = config.http_addr();
    let https_addr = config.https_addr();
    let tls_config = tls::rustls_config(config.tls_paths()).await?;

    let http_router = router(shared_state.clone(), TransportMeta::http());
    let https_router = router(shared_state, TransportMeta::https());

    let http_handle = Handle::new();
    let https_handle = Handle::new();

    let mut http_task = tokio::spawn(run_http(http_addr, http_router, http_handle.clone()));
    let mut https_task = tokio::spawn(run_https(
        https_addr,
        https_router,
        tls_config,
        https_handle.clone(),
    ));
    let mut shutdown = Box::pin(wait_for_shutdown(http_handle.clone(), https_handle.clone()));

    tokio::select! {
        res = &mut http_task => {
            res??;
            https_handle.shutdown();
        }
        res = &mut https_task => {
            res??;
            http_handle.shutdown();
        }
        _ = &mut shutdown => {
            tracing::info!("shutdown signal received");
        }
    }

    if !http_task.is_finished() {
        http_task.await??;
    }

    if !https_task.is_finished() {
        https_task.await??;
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

async fn wait_for_shutdown(http_handle: Handle, https_handle: Handle) {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(%error, "failed to install CTRL+C handler");
        return;
    }

    http_handle.shutdown();
    https_handle.shutdown();
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
