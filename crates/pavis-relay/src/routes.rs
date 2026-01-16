use crate::handlers::{
    get_artifact, get_config, get_health, get_metrics, get_ready, get_status, post_publish,
};
use crate::runtime::{RelayError, RelayRuntimeState};
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use std::net::SocketAddr;
use std::sync::Arc;

pub(crate) fn router(state: RelayRuntimeState) -> Router {
    let max_pvs_bytes = state.options().max_pvs_bytes;
    let shared = Arc::new(state);
    let mut app = Router::new()
        .route("/v1/config", get(get_config))
        .route("/v1/status", get(get_status))
        .route("/v1/publish", post(post_publish))
        .route("/v1/artifacts/:version", get(get_artifact))
        .route("/v1/metrics", get(get_metrics))
        .route("/health", get(get_health))
        .route("/ready", get(get_ready))
        .with_state(shared);

    if max_pvs_bytes > 0 {
        app = app.layer(DefaultBodyLimit::max(max_pvs_bytes as usize));
    }

    app
}

pub(crate) async fn serve(
    listen_addr: SocketAddr,
    state: RelayRuntimeState,
) -> Result<(), RelayError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RelayRuntimeState;
    use axum::body::Bytes;

    fn mock_state() -> RelayRuntimeState {
        RelayRuntimeState::new(0, Bytes::new()).expect("create state")
    }

    #[tokio::test]
    async fn test_router_construction() {
        let state = mock_state();
        let app = router(state);
        assert!(format!("{:?}", app).contains("Router"));
    }

    #[tokio::test]
    async fn test_serve_bind_error() {
        // Bind to a port first to occupy it
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping bind test: {err}");
                return;
            }
            Err(err) => panic!("failed to bind: {err}"),
        };
        let addr = listener.local_addr().unwrap();

        // Try to serve on the occupied address
        let state = mock_state();
        let result = serve(addr, state).await;

        assert!(result.is_err());
        match result {
            Err(RelayError::Http(msg)) => {
                let msg_lower = msg.to_lowercase();
                assert!(
                    msg_lower.contains("address already in use")
                        || msg_lower.contains("only one usage")
                        || msg.contains("ADDRINUSE")
                );
            }
            _ => panic!("Expected RelayError::Http, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_serve_can_start_and_abort() {
        let probe = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping serve test: {err}");
                return;
            }
            Err(err) => panic!("failed to bind: {err}"),
        };
        drop(probe);

        let state = mock_state();
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let handle = tokio::spawn(async move { serve(addr, state).await });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        handle.abort();
    }
}
