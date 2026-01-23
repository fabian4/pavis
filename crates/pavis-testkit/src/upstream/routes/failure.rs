use axum::{extract::State, http::StatusCode, response::Json};
use std::sync::atomic::Ordering;

use crate::upstream::routes::ServerState;

/// Handler for failure injection endpoint
///
/// Returns failure status codes based on attempt number according to
/// configured sequence. Used for deterministic retry testing.
pub async fn handler(
    State(state): State<ServerState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let attempt = state.shared.failure_counter.fetch_add(1, Ordering::SeqCst) + 1;

    tracing::debug!(attempt = attempt, "Failure injection request");

    let config = state.shared.failure_config.lock().expect("lock poisoned");

    // Check if this attempt should fail
    for rule in config.iter() {
        if rule.attempt == attempt {
            tracing::info!(attempt = attempt, status = rule.status, "Injecting failure");
            return Err(
                StatusCode::from_u16(rule.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            );
        }
    }

    // Success path
    Ok(Json(serde_json::json!({
        "instance_id": state.instance_id(),
        "attempt": attempt,
        "status": "success",
    })))
}
