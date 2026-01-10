use bytes::Bytes;
use http::header;
use http::{HeaderValue, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use pavis_benchkit::Metrics;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PORT: u16 = 8000;
const DEFAULT_FIXED_BYTES: usize = 64;
const DEFAULT_SLEEP_CAP_MS: u64 = 10_000;
const DEFAULT_WORKERS: usize = 2;
const HEALTHZ_BODY: &[u8] = b"ok";
const CONTENT_TYPE_OCTET_STREAM: &str = "application/octet-stream";
const CONTENT_TYPE_TEXT: &str = "text/plain";

struct AppState {
    fixed_payload: Bytes,
    fixed_len: HeaderValue,
    ok_body: Bytes,
    ok_len: HeaderValue,
    sleep_cap_ms: u64,
    metrics: Metrics,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = parse_env_u16("PORT", DEFAULT_PORT);
    let fixed_bytes = parse_env_usize("FIXED_BYTES", DEFAULT_FIXED_BYTES);
    let sleep_cap_ms = parse_env_u64("SLEEP_CAP_MS", DEFAULT_SLEEP_CAP_MS);
    let workers = parse_env_usize("WORKER_THREADS", DEFAULT_WORKERS);

    let fixed_payload = Bytes::from(vec![0u8; fixed_bytes]);
    let fixed_len = HeaderValue::from_str(&fixed_bytes.to_string())?;

    let ok_body = Bytes::from_static(HEALTHZ_BODY);
    let ok_len = HeaderValue::from_str(&HEALTHZ_BODY.len().to_string())?;

    let state = Arc::new(AppState {
        fixed_payload,
        fixed_len,
        ok_body,
        ok_len,
        sleep_cap_ms,
        metrics: Metrics::new(),
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_time()
        .build()?;

    if std::env::var("RUST_LOG").is_ok() {
        eprintln!(
            "bench-upstream listening on {addr}, fixed_bytes={fixed_bytes}, workers={workers}"
        );
    }

    runtime.block_on(async move {
        let make_svc = make_service_fn(move |_conn| {
            let state = state.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let state = state.clone();
                    handle_request(req, state)
                }))
            }
        });

        Server::bind(&addr)
            .http1_only(true)
            .http1_keepalive(true)
            .http1_half_close(false)
            .serve(make_svc)
            .await
            .map_err(|err| {
                if std::env::var("RUST_LOG").is_ok() {
                    eprintln!("bench-upstream server error: {err}");
                }
                err
            })
    })?;

    Ok(())
}

async fn handle_request(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, Infallible> {
    state.metrics.record_request();

    if req.method() != http::Method::GET {
        return Ok(respond_with(
            StatusCode::METHOD_NOT_ALLOWED,
            CONTENT_TYPE_OCTET_STREAM,
            state.fixed_payload.clone(),
            &state.fixed_len,
            should_close(&req),
        ));
    }

    let path = req.uri().path();
    let close = should_close(&req);

    let response = match path {
        "/healthz" => respond_with(
            StatusCode::OK,
            CONTENT_TYPE_TEXT,
            state.ok_body.clone(),
            &state.ok_len,
            close,
        ),
        "/fixed" => respond_with(
            StatusCode::OK,
            CONTENT_TYPE_OCTET_STREAM,
            state.fixed_payload.clone(),
            &state.fixed_len,
            close,
        ),
        "/metrics" => respond_metrics(&state, close),
        _ if path.starts_with("/status/") => {
            let status = parse_status(path).unwrap_or(StatusCode::BAD_REQUEST);
            respond_with(
                status,
                CONTENT_TYPE_OCTET_STREAM,
                state.fixed_payload.clone(),
                &state.fixed_len,
                close,
            )
        }
        "/sleep" => {
            let sleep_ms = parse_sleep_ms(req.uri().query(), state.sleep_cap_ms);
            match sleep_ms {
                Some(ms) => {
                    if ms > 0 {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                    }
                    respond_with(
                        StatusCode::OK,
                        CONTENT_TYPE_OCTET_STREAM,
                        state.fixed_payload.clone(),
                        &state.fixed_len,
                        close,
                    )
                }
                None => respond_with(
                    StatusCode::BAD_REQUEST,
                    CONTENT_TYPE_OCTET_STREAM,
                    state.fixed_payload.clone(),
                    &state.fixed_len,
                    close,
                ),
            }
        }
        _ => respond_with(
            StatusCode::NOT_FOUND,
            CONTENT_TYPE_OCTET_STREAM,
            state.fixed_payload.clone(),
            &state.fixed_len,
            close,
        ),
    };

    Ok(response)
}

fn respond_with(
    status: StatusCode,
    content_type: &str,
    body: Bytes,
    body_len: &HeaderValue,
    close: bool,
) -> Response<Body> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, body_len.clone());

    if close {
        builder = builder.header(header::CONNECTION, "close");
    }

    builder.body(Body::from(body)).unwrap_or_else(|_| {
        let mut fallback = Response::new(Body::from(Bytes::from_static(b"")));
        *fallback.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        fallback
            .headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        fallback
    })
}

fn respond_metrics(state: &AppState, close: bool) -> Response<Body> {
    #[cfg(feature = "metrics")]
    {
        let body = format!(
            "# HELP bench_upstream_requests_total Total HTTP requests.\n# TYPE bench_upstream_requests_total counter\nbench_upstream_requests_total {}\n",
            state.metrics.requests_total()
        );
        let len = HeaderValue::from_str(&body.len().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0"));
        respond_with(
            StatusCode::OK,
            "text/plain; version=0.0.4",
            Bytes::from(body),
            &len,
            close,
        )
    }

    #[cfg(not(feature = "metrics"))]
    {
        respond_with(
            StatusCode::NOT_FOUND,
            CONTENT_TYPE_OCTET_STREAM,
            state.fixed_payload.clone(),
            &state.fixed_len,
            close,
        )
    }
}

fn parse_status(path: &str) -> Option<StatusCode> {
    let code = path.strip_prefix("/status/")?;
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let parsed: u16 = code.parse().ok()?;
    if !(100..=599).contains(&parsed) {
        return None;
    }
    StatusCode::from_u16(parsed).ok()
}

fn parse_sleep_ms(query: Option<&str>, cap_ms: u64) -> Option<u64> {
    let query = query?;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == "ms" {
            let parsed: u64 = value.parse().ok()?;
            return Some(parsed.min(cap_ms));
        }
    }
    None
}

fn should_close(req: &Request<Body>) -> bool {
    req.headers()
        .get(header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("close"))
        })
        .unwrap_or(false)
}

fn parse_env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn parse_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;
    use http::header::{CONNECTION, CONTENT_LENGTH};
    #[cfg(feature = "metrics")]
    use hyper::body::HttpBody;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[test]
    fn parse_status_accepts_valid_range() {
        assert_eq!(parse_status("/status/200"), Some(StatusCode::OK));
        assert_eq!(
            parse_status("/status/599"),
            Some(StatusCode::from_u16(599).unwrap())
        );
        assert_eq!(parse_status("/status/100"), Some(StatusCode::CONTINUE));
    }

    #[test]
    fn parse_status_rejects_invalid_values() {
        assert_eq!(parse_status("/status/99"), None);
        assert_eq!(parse_status("/status/600"), None);
        assert_eq!(parse_status("/status/abc"), None);
        assert_eq!(parse_status("/status/"), None);
        assert_eq!(parse_status("/status/200/extra"), None);
    }

    #[test]
    fn parse_sleep_ms_reads_ms_and_caps() {
        assert_eq!(parse_sleep_ms(Some("ms=10"), 100), Some(10));
        assert_eq!(parse_sleep_ms(Some("ms=250"), 100), Some(100));
        assert_eq!(parse_sleep_ms(Some("foo=1&ms=5"), 100), Some(5));
    }

    #[test]
    fn parse_sleep_ms_rejects_missing_or_invalid() {
        assert_eq!(parse_sleep_ms(None, 100), None);
        assert_eq!(parse_sleep_ms(Some(""), 100), None);
        assert_eq!(parse_sleep_ms(Some("ms="), 100), None);
        assert_eq!(parse_sleep_ms(Some("ms=abc"), 100), None);
        assert_eq!(parse_sleep_ms(Some("ms=1&ms=2"), 100), Some(1));
    }

    #[test]
    fn should_close_detects_connection_close() {
        let req = Request::builder()
            .header(CONNECTION, "close")
            .body(Body::empty())
            .unwrap();
        assert!(should_close(&req));

        let req = Request::builder()
            .header(CONNECTION, "keep-alive")
            .body(Body::empty())
            .unwrap();
        assert!(!should_close(&req));
    }

    #[test]
    fn should_close_handles_comma_separated_values() {
        let req = Request::builder()
            .header(CONNECTION, "upgrade, close")
            .body(Body::empty())
            .unwrap();
        assert!(should_close(&req));
    }

    #[test]
    fn respond_with_sets_content_length() {
        let body = Bytes::from_static(b"abc");
        let len = HeaderValue::from_static("3");
        let response = respond_with(StatusCode::OK, CONTENT_TYPE_OCTET_STREAM, body, &len, false);
        assert_eq!(
            response.headers().get(CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("3")
        );
    }

    fn test_state(fixed_bytes: usize, sleep_cap_ms: u64) -> Arc<AppState> {
        let fixed_payload = Bytes::from(vec![0u8; fixed_bytes]);
        let fixed_len = HeaderValue::from_str(&fixed_bytes.to_string()).unwrap();
        let ok_body = Bytes::from_static(HEALTHZ_BODY);
        let ok_len = HeaderValue::from_str(&HEALTHZ_BODY.len().to_string()).unwrap();

        Arc::new(AppState {
            fixed_payload,
            fixed_len,
            ok_body,
            ok_len,
            sleep_cap_ms,
            metrics: Metrics::new(),
        })
    }

    #[tokio::test]
    async fn handle_request_healthz_ok() {
        let state = test_state(8, 10);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/healthz")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            CONTENT_TYPE_TEXT
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("2")
        );
    }

    #[tokio::test]
    async fn handle_request_fixed_len_matches() {
        let state = test_state(32, 10);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/fixed")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("32")
        );
    }

    #[tokio::test]
    async fn handle_request_status_invalid_returns_400() {
        let state = test_state(16, 10);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/status/999")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("16")
        );
    }

    #[tokio::test]
    async fn handle_request_sleep_with_ms() {
        let state = test_state(4, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/sleep?ms=1")
            .body(Body::empty())
            .unwrap();

        let resp = tokio::time::timeout(Duration::from_millis(50), handle_request(req, state))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("4")
        );
    }

    #[tokio::test]
    async fn handle_request_sleep_missing_ms_returns_400() {
        let state = test_state(4, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/sleep")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("4")
        );
    }

    #[tokio::test]
    async fn handle_request_non_get_returns_405() {
        let state = test_state(6, 5);
        let req = Request::builder()
            .method(http::Method::POST)
            .uri("/fixed")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("6")
        );
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn handle_request_metrics_enabled_returns_prometheus() {
        let state = test_state(8, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; version=0.0.4"
        );
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("bench_upstream_requests_total 1"));
    }

    #[cfg(not(feature = "metrics"))]
    #[tokio::test]
    async fn handle_request_metrics_disabled_returns_404() {
        let state = test_state(8, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("8")
        );
    }

    #[tokio::test]
    async fn handle_request_connection_close_echoes_header() {
        let state = test_state(8, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/fixed")
            .header(header::CONNECTION, "close")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONNECTION).unwrap(),
            &HeaderValue::from_static("close")
        );
    }

    #[tokio::test]
    async fn handle_request_status_valid_codes() {
        let state = test_state(5, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/status/503")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            CONTENT_TYPE_OCTET_STREAM
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("5")
        );
    }

    #[tokio::test]
    async fn handle_request_status_invalid_codes() {
        let state = test_state(7, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/status/abc")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("7")
        );
    }

    #[tokio::test]
    async fn handle_request_sleep_caps_ms() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/sleep?ms=50")
            .body(Body::empty())
            .unwrap();

        let resp = tokio::time::timeout(Duration::from_millis(50), handle_request(req, state))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("3")
        );
    }

    #[tokio::test]
    async fn handle_request_sleep_zero_ms_is_ok() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/sleep?ms=0")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn handle_request_fixed_content_type_is_octet_stream() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/fixed")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            CONTENT_TYPE_OCTET_STREAM
        );
    }

    #[tokio::test]
    async fn handle_request_close_on_error_paths() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/unknown")
            .header(header::CONNECTION, "close")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONNECTION).unwrap(),
            &HeaderValue::from_static("close")
        );
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn handle_request_metrics_has_content_length() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let content_length = resp
            .headers()
            .get(header::CONTENT_LENGTH)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let expected = resp.into_body().size_hint().lower().to_string();
        assert_eq!(content_length, expected);
    }

    #[tokio::test]
    async fn keepalive_reuses_connection_without_date_or_server() {
        let state = test_state(4, 5);
        let make_svc = make_service_fn(move |_conn| {
            let state = state.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let state = state.clone();
                    handle_request(req, state)
                }))
            }
        });

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = Server::from_tcp(listener)
            .unwrap()
            .http1_only(true)
            .http1_keepalive(true)
            .http1_half_close(false)
            .serve(make_svc);
        let server_task = tokio::spawn(server);

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = b"GET /fixed HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(request).await.unwrap();
        let response = read_http_response(&mut stream).await;
        assert_eq!(response.status, StatusCode::OK);
        assert!(response.header_present("date"));
        assert!(!response.header_present("server"));
        assert!(!response.header_present("connection"));

        stream.write_all(request).await.unwrap();
        let response = read_http_response(&mut stream).await;
        assert_eq!(response.status, StatusCode::OK);

        server_task.abort();
    }

    struct RawResponse {
        status: StatusCode,
        headers: Vec<(String, String)>,
    }

    impl RawResponse {
        fn header_present(&self, name: &str) -> bool {
            self.headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name))
        }
    }

    async fn read_http_response(stream: &mut TcpStream) -> RawResponse {
        let mut buffer = Vec::new();
        let mut temp = [0u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut temp).await.unwrap();
            assert!(n > 0, "connection closed before headers");
            buffer.extend_from_slice(&temp[..n]);
            if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let headers_bytes = &buffer[..header_end];
        let headers_text = std::str::from_utf8(headers_bytes).unwrap();
        let mut lines = headers_text.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut headers = Vec::new();
        let mut content_len = 0usize;
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                if key.eq_ignore_ascii_case("content-length") {
                    content_len = value.parse().unwrap_or(0);
                }
                headers.push((key, value));
            }
        }

        let mut body_len = buffer.len() - header_end;
        while body_len < content_len {
            let n = stream.read(&mut temp).await.unwrap();
            assert!(n > 0, "connection closed before body");
            body_len += n;
        }

        RawResponse {
            status: status_code,
            headers,
        }
    }
}
