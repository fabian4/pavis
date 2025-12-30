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

#[cfg(test)]
mod tests {
    use super::{apply_request_headers, apply_response_headers};
    use pavis_core::HeaderOperations;
    use pingora::http::{RequestHeader, ResponseHeader};

    #[test]
    fn test_apply_headers() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Remove", "old-value").unwrap();

        let ops = HeaderOperations {
            add: vec![("X-Add".to_string(), "new-value".to_string())],
            remove: vec!["X-Remove".to_string()],
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        assert_eq!(
            req.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
            "Pavis"
        );
        assert_eq!(
            req.headers.get("X-Add").unwrap().to_str().unwrap(),
            "new-value"
        );
        assert!(req.headers.get("X-Remove").is_none());
    }

    #[test]
    fn test_apply_response_headers() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        resp.insert_header("X-Remove-Resp", "bad-value").unwrap();

        let ops = HeaderOperations {
            add: vec![("X-Add-Resp".to_string(), "good-value".to_string())],
            remove: vec!["X-Remove-Resp".to_string()],
        };

        apply_response_headers(&mut resp, Some(&ops)).unwrap();

        assert_eq!(
            resp.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
            "Pavis"
        );
        assert_eq!(
            resp.headers.get("X-Add-Resp").unwrap().to_str().unwrap(),
            "good-value"
        );
        assert!(resp.headers.get("X-Remove-Resp").is_none());
    }

    #[test]
    fn test_apply_request_headers_skips_invalid_entries() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        let ops = HeaderOperations {
            add: vec![
                ("bad header".to_string(), "ok".to_string()),
                ("x-bad-value".to_string(), "bad\nvalue".to_string()),
            ],
            remove: vec![],
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        assert!(req.headers.get("bad header").is_none());
        assert!(req.headers.get("x-bad-value").is_none());
    }

    #[test]
    fn test_apply_response_headers_skips_invalid_entries() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        let ops = HeaderOperations {
            add: vec![
                ("bad header".to_string(), "ok".to_string()),
                ("x-bad-value".to_string(), "bad\nvalue".to_string()),
            ],
            remove: vec![],
        };

        apply_response_headers(&mut resp, Some(&ops)).unwrap();

        assert!(resp.headers.get("bad header").is_none());
        assert!(resp.headers.get("x-bad-value").is_none());
    }
}
