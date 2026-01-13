use crate::common::cli::RelayArgs;
use crate::relay::state::RelayState;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
pub struct LongPollQuery {
    etag: Option<String>,
    timeout_ms: Option<u64>,
}

pub async fn handler(
    State(state): State<RelayState>,
    State(args): State<RelayArgs>,
    Query(params): Query<LongPollQuery>,
) -> Response {
    let client_etag = params.etag.as_deref().unwrap_or("");
    let timeout_val = params.timeout_ms.unwrap_or(args.default_timeout_ms);
    let timeout_dur = Duration::from_millis(timeout_val);

    // Check immediate
    #[allow(clippy::collapsible_if)]
    if let Some((meta, data)) = state.get_current().await {
        if meta.etag != client_etag {
            let mut headers = HeaderMap::new();
            headers.insert(header::ETAG, meta.etag.parse().unwrap());
            headers.insert("x-pavis-version", meta.rev.to_string().parse().unwrap());
            headers.insert(
                header::CONTENT_TYPE,
                "application/octet-stream".parse().unwrap(),
            );
            return (headers, data).into_response();
        }
    }

    // Wait
    let mut rx = state.subscribe();
    match tokio::time::timeout(timeout_dur, rx.changed()).await {
        Ok(Ok(())) => {
            // Changed!
            if let Some((meta, data)) = state.get_current().await {
                let mut headers = HeaderMap::new();
                headers.insert(header::ETAG, meta.etag.parse().unwrap());
                headers.insert("x-pavis-version", meta.rev.to_string().parse().unwrap());
                headers.insert(
                    header::CONTENT_TYPE,
                    "application/octet-stream".parse().unwrap(),
                );
                return (headers, data).into_response();
            }
            // Should not happen if changed() returned, unless it was cleared?
            (StatusCode::NOT_MODIFIED, "").into_response()
        }
        _ => {
            // Timeout or error
            (StatusCode::NOT_MODIFIED, "").into_response()
        }
    }
}
