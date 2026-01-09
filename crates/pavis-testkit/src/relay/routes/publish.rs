use crate::common::cli::RelayArgs;
use crate::relay::state::RelayState;
use axum::{
    body::{self, Body},
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

pub async fn handler(
    State(state): State<RelayState>,
    State(args): State<RelayArgs>,
    request: Request<Body>,
) -> Response {
    let limit = args.max_body;
    let body_bytes = match body::to_bytes(request.into_body(), limit).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };

    let meta = state.publish(body_bytes).await;
    tracing::info!(rev = meta.rev, size = meta.size, "published artifact");

    (StatusCode::OK, Json(meta)).into_response()
}
