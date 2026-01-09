use super::TestContext;
use crate::upstream::types::DelayResponse;
use axum::{extract::Query, http::StatusCode, response::Response};
use serde::Deserialize;
use std::time::Duration;

const MAX_DELAY_MS: u64 = 60_000;

#[derive(Deserialize)]
pub struct DelayQuery {
    ms: Option<u64>,
}

pub async fn handler(Query(params): Query<DelayQuery>, ctx: TestContext) -> Response {
    let requested = params.ms.unwrap_or(0);
    let bounded = requested.min(MAX_DELAY_MS);
    tokio::time::sleep(Duration::from_millis(bounded)).await;

    ctx.respond(
        StatusCode::OK,
        DelayResponse {
            delayed_ms: bounded,
        },
    )
}
