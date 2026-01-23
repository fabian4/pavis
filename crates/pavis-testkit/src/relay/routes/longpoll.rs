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
    wait_ms: Option<u64>,
}

fn parse_if_none_match(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::IF_NONE_MATCH)?;
    let s = value.to_str().ok()?;
    let trimmed = s.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Some(trimmed.to_string())
    }
}

pub async fn handler(
    State(state): State<RelayState>,
    State(args): State<RelayArgs>,
    Query(params): Query<LongPollQuery>,
    headers: HeaderMap,
) -> Response {
    let client_etag = parse_if_none_match(&headers).unwrap_or_default();
    let timeout_val = params.wait_ms.unwrap_or(args.default_timeout_ms);
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
    let etag = match HeaderValue::from_str(&format!("\"{}\"", meta.etag)) {
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
    headers.insert("x-config-version", version);
    headers.insert(
        "x-config-size",
        HeaderValue::from_str(&meta.size.to_string()).unwrap(),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, data).into_response()
}
