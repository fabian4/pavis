use http::header::{HeaderName, HeaderValue};
use pavis_core::HeaderOperations;
use pingora::Result;
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use std::str::FromStr;

pub fn apply_request_headers(
    req: &mut RequestHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    req.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        for (k, v) in &headers.add {
            match (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                (Ok(key), Ok(val)) => {
                    req.insert_header(key, val)?;
                }
                (Err(e), _) => {
                    tracing::warn!("Invalid request header name '{:?}': {}", k, e);
                }
                (_, Err(e)) => {
                    tracing::warn!("Invalid request header value for '{:?}': {}", k, e);
                }
            }
        }
        for k in &headers.remove {
            req.remove_header(k);
        }
    }
    Ok(())
}

pub fn apply_response_headers(
    resp: &mut ResponseHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    resp.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        for (k, v) in &headers.add {
            match (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                (Ok(key), Ok(val)) => {
                    resp.insert_header(key, val)?;
                }
                (Err(e), _) => {
                    tracing::warn!("Invalid response header name '{:?}': {}", k, e);
                }
                (_, Err(e)) => {
                    tracing::warn!("Invalid response header value for '{:?}': {}", k, e);
                }
            }
        }
        for k in &headers.remove {
            resp.remove_header(k);
        }
    }
    Ok(())
}
