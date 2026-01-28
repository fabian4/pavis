use crate::relay::state::RelayState;
use axum::{Json, extract::State};

pub async fn handler(
    State(state): State<RelayState>,
) -> Json<Vec<crate::relay::state::RequestRecord>> {
    Json(state.get_requests().await)
}
