use crate::common::cli::RelayArgs;
use crate::relay::state::RelayState;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
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
    if let Some((meta, data)) = state.get_current().await
        && meta.etag != client_etag
    {
        return response_with_meta(&meta, data);
    }

    // Wait
    let mut rx = state.subscribe();
    match tokio::time::timeout(timeout_dur, rx.changed()).await {
        Ok(Ok(())) => {
            // Changed!
            if let Some((meta, data)) = state.get_current().await {
                return response_with_meta(&meta, data);
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

fn response_with_meta(meta: &crate::relay::types::ArtifactMeta, data: bytes::Bytes) -> Response {
    let mut headers = HeaderMap::new();
    let etag = match HeaderValue::from_str(&meta.etag) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = %err, "invalid etag header");
            return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
        }
    };
    let version = match HeaderValue::from_str(&meta.rev.to_string()) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = %err, "invalid version header");
            return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
        }
    };
    headers.insert(header::ETAG, etag);
    headers.insert("x-pavis-version", version);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    (headers, data).into_response()
}
