use super::TestContext;
use crate::upstream::types::HealthResponse;
use axum::{http::StatusCode, response::Response};

pub async fn handler(ctx: TestContext) -> Response {
    ctx.respond(StatusCode::OK, HealthResponse { ok: true })
}
