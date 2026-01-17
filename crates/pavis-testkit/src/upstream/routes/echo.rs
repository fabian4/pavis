use super::{RemoteAddress, ServerState, TestContext};
use crate::upstream::types::{EchoResponse, StubResponse, TlsDetails};
use axum::{
    body::{self, Body},
    extract::State,
    http::{HeaderMap, Request, StatusCode, Version},
    response::Response,
};
use std::collections::BTreeMap;

const MAX_ECHO_BODY: usize = 1024 * 1024;

pub async fn handler(
    State(state): State<ServerState>,
    RemoteAddress(remote_addr): RemoteAddress,
    ctx: TestContext,
    request: Request<Body>,
) -> Response {
    // Apply global delay if configured
    if let Some(delay_ms) = state.shared.global_delay_ms() {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }

    let method = request.method().clone();
    let uri = request.uri().clone();
    let version = request.version();
    let headers = request.headers().clone();

    let body_bytes = match body::to_bytes(request.into_body(), MAX_ECHO_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return ctx.respond(
                StatusCode::PAYLOAD_TOO_LARGE,
                StubResponse {
                    error: "body_too_large",
                    endpoint: "/echo",
                    note: "payload exceeded limit",
                },
            );
        }
    };

    let response = EchoResponse {
        instance_id: state.instance_id().to_string(),
        method: method.to_string(),
        path: uri.path().to_string(),
        query: uri.query().unwrap_or_default().to_string(),
        protocol: version_string(version),
        tls: TlsDetails {
            enabled: state.transport().tls_enabled(),
            version: None,
            sni: None,
        },
        headers: canonical_headers(&headers),
        body_len: body_bytes.len(),
        remote_addr: remote_addr.map(|addr| addr.to_string()),
    };

    ctx.respond(StatusCode::OK, response)
}

fn canonical_headers(headers: &HeaderMap) -> BTreeMap<String, Vec<String>> {
    let mut canonical = BTreeMap::new();

    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        let entry = canonical.entry(key).or_insert_with(Vec::new);
        match value.to_str() {
            Ok(text) => entry.push(text.to_string()),
            Err(_) => entry.push(String::new()),
        }
    }

    canonical
}

fn version_string(version: Version) -> Option<String> {
    match version {
        Version::HTTP_09 => Some("HTTP/0.9".to_string()),
        Version::HTTP_10 => Some("HTTP/1.0".to_string()),
        Version::HTTP_11 => Some("HTTP/1.1".to_string()),
        Version::HTTP_2 => Some("HTTP/2".to_string()),
        Version::HTTP_3 => Some("HTTP/3".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_canonical_headers_normalizes_case() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("X-Custom", "Value".parse().unwrap());

        let canonical = canonical_headers(&headers);
        assert!(canonical.contains_key("content-type"));
        assert!(canonical.contains_key("x-custom"));
        assert!(!canonical.contains_key("Content-Type"));
    }

    #[test]
    fn test_version_string() {
        assert_eq!(
            version_string(Version::HTTP_11),
            Some("HTTP/1.1".to_string())
        );
        assert_eq!(version_string(Version::HTTP_2), Some("HTTP/2".to_string()));
        // Note: Axum/Hyper might have more variants or different mapping
    }
}
