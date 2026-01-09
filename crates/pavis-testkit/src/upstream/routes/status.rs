use super::TestContext;
use crate::upstream::types::StatusResponse;
use axum::{extract::Query, http::StatusCode, response::Response};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct StatusQuery {
    code: Option<u16>,
}

pub async fn handler(Query(params): Query<StatusQuery>, ctx: TestContext) -> Response {
    let code = params.code.unwrap_or(StatusCode::OK.as_u16());
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    let ok = status.is_success();

    ctx.respond(
        status,
        StatusResponse {
            status: status.as_u16(),
            ok,
        },
    )
}
