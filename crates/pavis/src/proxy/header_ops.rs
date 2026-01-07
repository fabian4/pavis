use http::header::{HeaderName, HeaderValue};
use pavis_core::{HeaderName as CoreHeaderName, HeaderValue as CoreHeaderValue, HeadersPolicy};
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

fn apply_set<H: HeaderEditor>(
    headers: &mut H,
    name: &CoreHeaderName,
    value: &CoreHeaderValue,
    scope: &str,
) -> Result<()> {
    match (
        HeaderName::from_str(&name.0),
        HeaderValue::from_str(&value.0),
    ) {
        (Ok(key), Ok(val)) => headers.insert(key, val),
        (Err(e), _) => {
            tracing::warn!("Invalid {} header name '{:?}': {}", scope, name.0, e);
            Ok(())
        }
        (_, Err(e)) => {
            tracing::warn!("Invalid {} header value for '{:?}': {}", scope, name.0, e);
            Ok(())
        }
    }
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

pub fn apply_request_headers(req: &mut RequestHeader, headers: &HeadersPolicy) -> Result<()> {
    req.insert_header("X-Proxy-By", "Pavis")?;

    let rules = match headers {
        HeadersPolicy::Disabled => return Ok(()),
        HeadersPolicy::Enabled { rules } => rules,
        #[allow(unreachable_patterns)]
        &_ => return Ok(()),
    };

    for (name, value) in &rules.set_headers {
        apply_set(req, name, value, "request")?;
    }
    for (name, value) in &rules.append_headers {
        apply_append(req, &name.0, &value.0, "request")?;
    }
    for (name, value) in &rules.add_headers {
        if req.headers.get(name.0.as_str()).is_none() {
            apply_set(req, name, value, "request")?;
        }
    }
    for name in &rules.remove_headers {
        req.remove_header(name.0.as_str());
    }
    Ok(())
}

pub fn apply_response_headers(resp: &mut ResponseHeader, headers: &HeadersPolicy) -> Result<()> {
    resp.insert_header("X-Proxy-By", "Pavis")?;

    let rules = match headers {
        HeadersPolicy::Disabled => return Ok(()),
        HeadersPolicy::Enabled { rules } => rules,
        #[allow(unreachable_patterns)]
        &_ => return Ok(()),
    };

    for (name, value) in &rules.set_headers {
        apply_set(resp, name, value, "response")?;
    }
    for (name, value) in &rules.append_headers {
        apply_append(resp, &name.0, &value.0, "response")?;
    }
    for (name, value) in &rules.add_headers {
        if resp.headers.get(name.0.as_str()).is_none() {
            apply_set(resp, name, value, "response")?;
        }
    }
    for name in &rules.remove_headers {
        resp.remove_header(name.0.as_str());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_request_headers, apply_response_headers};
    use pavis_core::{HeaderName, HeaderValue, Headers, HeadersPolicy};
    use pingora::http::{RequestHeader, ResponseHeader};

    #[test]
    fn test_apply_headers() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Remove", "old-value").unwrap();

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("X-Add".to_string()),
                    HeaderValue("new-value".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: vec![HeaderName("X-Remove".to_string())],
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();

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

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("X-Add-Resp".to_string()),
                    HeaderValue("good-value".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: vec![HeaderName("X-Remove-Resp".to_string())],
            },
        };

        apply_response_headers(&mut resp, &ops).unwrap();

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
        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![
                    (
                        HeaderName("bad header".to_string()),
                        HeaderValue("ok".to_string()),
                    ),
                    (
                        HeaderName("x-bad-value".to_string()),
                        HeaderValue("bad\nvalue".to_string()),
                    ),
                ],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();

        assert!(req.headers.get("bad header").is_none());
        assert!(req.headers.get("x-bad-value").is_none());
    }

    #[test]
    fn test_apply_response_headers_skips_invalid_entries() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![
                    (
                        HeaderName("bad header".to_string()),
                        HeaderValue("ok".to_string()),
                    ),
                    (
                        HeaderName("x-bad-value".to_string()),
                        HeaderValue("bad\nvalue".to_string()),
                    ),
                ],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_response_headers(&mut resp, &ops).unwrap();

        assert!(resp.headers.get("bad header").is_none());
        assert!(resp.headers.get("x-bad-value").is_none());
    }

    #[test]
    fn test_append_joinable_request_collapses_lines() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Test", "a").unwrap();
        req.append_header("X-Test", "b").unwrap();

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: vec![(
                    HeaderName("X-Test".to_string()),
                    HeaderValue("c".to_string()),
                )],
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();

        let values: Vec<_> = req.headers.get_all("X-Test").iter().collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].to_str().unwrap(), "a, b, c");
    }

    #[test]
    fn test_append_non_joinable_set_cookie_keeps_lines() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("Set-Cookie", "a=1").unwrap();

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: vec![(
                    HeaderName("set-cookie".to_string()),
                    HeaderValue("b=2".to_string()),
                )],
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();

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

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: vec![(
                    HeaderName("x-test".to_string()),
                    HeaderValue("three".to_string()),
                )],
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_response_headers(&mut resp, &ops).unwrap();

        let values: Vec<_> = resp.headers.get_all("X-Test").iter().collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].to_str().unwrap(), "one, two, three");
    }

    #[test]
    fn test_apply_headers_disabled() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        apply_request_headers(&mut req, &HeadersPolicy::Disabled).unwrap();
        assert_eq!(
            req.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
            "Pavis"
        );
    }

    #[test]
    fn test_add_headers_skips_if_exists() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        req.insert_header("X-Exists", "original").unwrap();

        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: Vec::new(),
                add_headers: vec![(
                    HeaderName("X-Exists".to_string()),
                    HeaderValue("new".to_string()),
                )],
                remove_headers: Vec::new(),
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();
        assert_eq!(
            req.headers.get("X-Exists").unwrap().to_str().unwrap(),
            "original"
        );
    }

    #[test]
    fn test_apply_append_no_existing() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        let ops = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: vec![(
                    HeaderName("X-New".to_string()),
                    HeaderValue("value".to_string()),
                )],
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };

        apply_request_headers(&mut req, &ops).unwrap();
        assert_eq!(req.headers.get("X-New").unwrap().to_str().unwrap(), "value");
    }

    #[test]
    fn test_apply_set_invalid_inputs() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        // Invalid name (contains space)
        super::apply_set(
            &mut req,
            &HeaderName("Bad Name".to_string()),
            &HeaderValue("v".to_string()),
            "test",
        )
        .unwrap();
        assert!(req.headers.get("Bad Name").is_none());

        // Invalid value (contains newline)
        super::apply_set(
            &mut req,
            &HeaderName("X-Valid".to_string()),
            &HeaderValue("bad\nvalue".to_string()),
            "test",
        )
        .unwrap();
        assert!(req.headers.get("X-Valid").is_none());
    }

    #[test]
    fn test_apply_append_invalid_inputs() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        // Invalid name
        super::apply_append(&mut req, "Bad Name", "v", "test").unwrap();
        // Invalid value
        super::apply_append(&mut req, "X-Valid", "bad\nvalue", "test").unwrap();
        assert!(req.headers.get("X-Valid").is_none());
    }

    #[test]
    fn test_build_joined_value_empty() {
        let val = http::HeaderValue::from_static("v");
        let res = super::build_joined_value(std::iter::empty(), &val);
        assert!(res.is_none());
    }
}
