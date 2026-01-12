pub mod longpoll;
pub mod publish;
pub mod status;

use crate::relay::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn router(app_state: AppState) -> Router {
    Router::new()
        .route("/publish", post(publish::handler))
        .route("/v1/config", get(longpoll::handler))
        .route("/status", get(status::handler))
        .with_state(app_state)
}
