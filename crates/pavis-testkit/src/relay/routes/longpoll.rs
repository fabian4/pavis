use crate::common::cli::RelayArgs;
use crate::relay::state::{MockMode, RelayState};
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
    let client_if_none_match = parse_if_none_match(&headers);
    let client_etag = client_if_none_match.clone().unwrap_or_default();
    let timeout_val = params.wait_ms.unwrap_or(args.default_timeout_ms);
    let timeout_dur = Duration::from_millis(timeout_val);
    state
        .record_request(params.wait_ms, client_if_none_match.clone())
        .await;

    if let Some(mode) = args.mode.as_deref().and_then(MockMode::parse) {
        match mode {
            MockMode::ResyncOnce => {
                let attempt = state.next_script_attempt();
                // Return 410 only on the very first request, then normal processing
                if attempt == 0 {
                    return (StatusCode::GONE, "").into_response();
                }
                // Fall through to normal processing after first 410
            }
            MockMode::CorruptOnce => {
                let attempt = state.next_script_attempt();
                if attempt == 0 {
                    return response_with_bytes(corrupt_bytes(), 1);
                }
                // Fall through to normal processing after first corrupt response
            }
            MockMode::CorruptRepeat => {
                // Always return corrupt bytes, regardless of attempt count
                // Increment counter to ensure proper request tracking in all environments
                let _ = state.next_script_attempt();
                return response_with_bytes(corrupt_bytes(), 1);
            }
        }
    }

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

fn response_with_bytes(data: bytes::Bytes, rev: u64) -> Response {
    let mut headers = HeaderMap::new();
    let checksum = checksum_for_bytes(&data);
    let etag = match HeaderValue::from_str(&format!("\"{}\"", checksum)) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = %err, "invalid etag header");
            return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
        }
    };
    headers.insert(header::ETAG, etag);
    headers.insert(
        "x-config-version",
        HeaderValue::from_str(&rev.to_string()).unwrap(),
    );
    headers.insert(
        "x-config-size",
        HeaderValue::from_str(&data.len().to_string()).unwrap(),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, data).into_response()
}

fn corrupt_bytes() -> bytes::Bytes {
    bytes::Bytes::from(vec![0u8; 100])
}

fn checksum_for_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = pavis_pvs::compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
