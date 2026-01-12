use crate::relay::state::RelayState;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub async fn handler(State(state): State<RelayState>) -> Response {
    match state.get_meta().await {
        Some(meta) => (StatusCode::OK, Json(meta)).into_response(),
        None => (StatusCode::NOT_FOUND, "no artifact").into_response(),
    }
}
