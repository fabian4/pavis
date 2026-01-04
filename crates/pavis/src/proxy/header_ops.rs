use http::header::{HeaderName, HeaderValue};
use pavis_core::HeaderOperations;
use pingora::Result;
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use std::str::FromStr;

const NON_JOINABLE_HEADERS: [&str; 1] = ["set-cookie"];

trait HeaderEditor {
    fn header_map(&self) -> &http::HeaderMap;
    fn remove_all(&mut self, name: &HeaderName);
    fn insert(&mut self, name: HeaderName, value: HeaderValue) -> Result<()>;
    fn append(&mut self, name: HeaderName, value: HeaderValue) -> Result<()>;
}

impl HeaderEditor for RequestHeader {
    fn header_map(&self) -> &http::HeaderMap {
        &self.headers
    }

    fn remove_all(&mut self, name: &HeaderName) {
        let _ = RequestHeader::remove_header(self, name.as_str());
    }

    fn insert(&mut self, name: HeaderName, value: HeaderValue) -> Result<()> {
        RequestHeader::insert_header(self, name, value)
    }

    fn append(&mut self, name: HeaderName, value: HeaderValue) -> Result<()> {
        RequestHeader::append_header(self, name, value).map(|_| ())
    }
}

impl HeaderEditor for ResponseHeader {
    fn header_map(&self) -> &http::HeaderMap {
        &self.headers
    }

    fn remove_all(&mut self, name: &HeaderName) {
        let _ = ResponseHeader::remove_header(self, name.as_str());
    }

    fn insert(&mut self, name: HeaderName, value: HeaderValue) -> Result<()> {
        ResponseHeader::insert_header(self, name, value)
    }

    fn append(&mut self, name: HeaderName, value: HeaderValue) -> Result<()> {
        ResponseHeader::append_header(self, name, value).map(|_| ())
    }
}

fn is_non_joinable(name: &str) -> bool {
    NON_JOINABLE_HEADERS
        .iter()
        .any(|header| name.eq_ignore_ascii_case(header))
}

fn build_joined_value<'a, I>(values: I, appended: &HeaderValue) -> Option<Vec<u8>>
where
    I: Iterator<Item = &'a HeaderValue>,
{
    let mut buf = Vec::new();
    let mut saw_existing = false;

    for value in values {
        if saw_existing {
            buf.extend_from_slice(b", ");
        }
        buf.extend_from_slice(value.as_bytes());
        saw_existing = true;
    }

    if !saw_existing {
        return None;
    }

    buf.extend_from_slice(b", ");
    buf.extend_from_slice(appended.as_bytes());
    Some(buf)
}

fn apply_append<H: HeaderEditor>(
    headers: &mut H,
    key: &str,
    value: &str,
    scope: &str,
) -> Result<()> {
    let is_non_joinable = is_non_joinable(key);
    match (HeaderName::from_str(key), HeaderValue::from_str(value)) {
        (Ok(header_name), Ok(header_value)) => {
            if is_non_joinable {
                headers.append(header_name, header_value)?;
                return Ok(());
            }

            let existing = headers.header_map().get_all(&header_name);
            if existing.iter().next().is_none() {
                headers.insert(header_name, header_value)?;
                return Ok(());
            }

            let joined = match build_joined_value(existing.iter(), &header_value) {
                Some(buf) => match HeaderValue::from_bytes(&buf) {
                    Ok(val) => val,
                    Err(err) => {
                        tracing::warn!(
                            scope = %scope,
                            header = %key,
                            error = %err,
                            "Invalid joined header value"
                        );
                        return Ok(());
                    }
                },
                None => {
                    headers.insert(header_name, header_value)?;
                    return Ok(());
                }
            };

            headers.remove_all(&header_name);
            headers.insert(header_name, joined)?;
        }
        (Err(e), _) => {
            tracing::warn!("Invalid {} header name '{:?}': {}", scope, key, e);
        }
        (_, Err(e)) => {
            tracing::warn!("Invalid {} header value for '{:?}': {}", scope, key, e);
        }
    }
    Ok(())
}

pub fn apply_request_headers(
    req: &mut RequestHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    req.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        for action in &headers.actions {
            let k = &action.key;
            let v = action.value.as_deref().unwrap_or("");

            match action.action {
                pavis_core::HeaderActionType::Set => {
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
                pavis_core::HeaderActionType::Append => {
                    apply_append(req, k, v, "request")?;
                }
                pavis_core::HeaderActionType::AddIfAbsent => {
                    if req.headers.get(k).is_none() {
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
                }
                pavis_core::HeaderActionType::Remove => {
                    req.remove_header(k);
                }
            }
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
        for action in &headers.actions {
            let k = &action.key;
            let v = action.value.as_deref().unwrap_or("");

            match action.action {
                pavis_core::HeaderActionType::Set => {
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
                pavis_core::HeaderActionType::Append => {
                    apply_append(resp, k, v, "response")?;
                }
                pavis_core::HeaderActionType::AddIfAbsent => {
                    if resp.headers.get(k).is_none() {
                        match (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                            (Ok(key), Ok(val)) => {
                                resp.insert_header(key, val)?;
                            }
                            (Err(e), _) => {
                                tracing::warn!("Invalid response header name '{:?}': {}", k, e);
                            }
                            (_, Err(e)) => {
                                tracing::warn!(
                                    "Invalid response header value for '{:?}': {}",
                                    k,
                                    e
                                );
                            }
                        }
                    }
                }
                pavis_core::HeaderActionType::Remove => {
                    resp.remove_header(k);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_request_headers, apply_response_headers};
    use pavis_core::{HeaderAction, HeaderActionType, HeaderOperations};
    use pingora::http::{RequestHeader, ResponseHeader};

    #[test]
    fn test_apply_headers() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Remove", "old-value").unwrap();

        let ops = HeaderOperations {
            actions: vec![
                HeaderAction {
                    key: "X-Add".to_string(),
                    value: Some("new-value".to_string()),
                    action: HeaderActionType::Set,
                },
                HeaderAction {
                    key: "X-Remove".to_string(),
                    value: None,
                    action: HeaderActionType::Remove,
                },
            ],
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
            actions: vec![
                HeaderAction {
                    key: "X-Add-Resp".to_string(),
                    value: Some("good-value".to_string()),
                    action: HeaderActionType::Set,
                },
                HeaderAction {
                    key: "X-Remove-Resp".to_string(),
                    value: None,
                    action: HeaderActionType::Remove,
                },
            ],
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
            actions: vec![
                HeaderAction {
                    key: "bad header".to_string(),
                    value: Some("ok".to_string()),
                    action: HeaderActionType::Set,
                },
                HeaderAction {
                    key: "x-bad-value".to_string(),
                    value: Some("bad\nvalue".to_string()),
                    action: HeaderActionType::Set,
                },
            ],
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        assert!(req.headers.get("bad header").is_none());
        assert!(req.headers.get("x-bad-value").is_none());
    }

    #[test]
    fn test_apply_response_headers_skips_invalid_entries() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        let ops = HeaderOperations {
            actions: vec![
                HeaderAction {
                    key: "bad header".to_string(),
                    value: Some("ok".to_string()),
                    action: HeaderActionType::Set,
                },
                HeaderAction {
                    key: "x-bad-value".to_string(),
                    value: Some("bad\nvalue".to_string()),
                    action: HeaderActionType::Set,
                },
            ],
        };

        apply_response_headers(&mut resp, Some(&ops)).unwrap();

        assert!(resp.headers.get("bad header").is_none());
        assert!(resp.headers.get("x-bad-value").is_none());
    }

    #[test]
    fn test_append_joinable_request_collapses_lines() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Test", "a").unwrap();
        req.append_header("X-Test", "b").unwrap();

        let ops = HeaderOperations {
            actions: vec![HeaderAction {
                key: "X-Test".to_string(),
                value: Some("c".to_string()),
                action: HeaderActionType::Append,
            }],
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        let values: Vec<_> = req.headers.get_all("X-Test").iter().collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].to_str().unwrap(), "a, b, c");
    }

    #[test]
    fn test_append_non_joinable_set_cookie_keeps_lines() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("Set-Cookie", "a=1").unwrap();

        let ops = HeaderOperations {
            actions: vec![HeaderAction {
                key: "set-cookie".to_string(),
                value: Some("b=2".to_string()),
                action: HeaderActionType::Append,
            }],
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        let values: Vec<_> = req
            .headers
            .get_all("Set-Cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(values, vec!["a=1", "b=2"]);
    }

    #[test]
    fn test_append_joinable_response_collapses_lines() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        resp.insert_header("X-Test", "one").unwrap();
        resp.append_header("X-Test", "two").unwrap();

        let ops = HeaderOperations {
            actions: vec![HeaderAction {
                key: "x-test".to_string(),
                value: Some("three".to_string()),
                action: HeaderActionType::Append,
            }],
        };

        apply_response_headers(&mut resp, Some(&ops)).unwrap();

        let values: Vec<_> = resp.headers.get_all("X-Test").iter().collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].to_str().unwrap(), "one, two, three");
    }
}
