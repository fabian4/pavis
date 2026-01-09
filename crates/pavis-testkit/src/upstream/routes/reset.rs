use super::TestContext;
use crate::upstream::types::StubResponse;
use axum::{http::StatusCode, response::Response};

pub async fn handler(ctx: TestContext) -> Response {
    ctx.respond(
        StatusCode::NOT_IMPLEMENTED,
        StubResponse {
            error: "not_implemented",
            endpoint: "/reset",
            note: "TODO",
        },
    )
}
