use crate::handlers::{
    get_artifact, get_config, get_health, get_metrics, get_ready, get_status, post_publish,
};
use crate::{RelayError, RelayState};
use axum::Router;
use axum::routing::{get, post};
use std::net::SocketAddr;
use std::sync::Arc;

pub fn router(state: RelayState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/v1/config", get(get_config))
        .route("/v1/status", get(get_status))
        .route("/v1/publish", post(post_publish))
        .route("/v1/artifacts/:version", get(get_artifact))
        .route("/v1/metrics", get(get_metrics))
        .route("/health", get(get_health))
        .route("/ready", get(get_ready))
        .with_state(shared)
}

pub async fn serve(listen_addr: SocketAddr, state: RelayState) -> Result<(), RelayError> {
    let app = router(state);
    axum::serve(
        tokio::net::TcpListener::bind(listen_addr)
            .await
            .map_err(|e| RelayError::Http(e.to_string()))?,
        app,
    )
    .await
    .map_err(|e| RelayError::Http(e.to_string()))
}
