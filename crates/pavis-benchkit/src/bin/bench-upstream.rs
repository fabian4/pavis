use bytes::Bytes;
use http::header;
use http::{HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

const DEFAULT_PORT: u16 = 8000;
const DEFAULT_FIXED_BYTES: usize = 64;
const DEFAULT_SLEEP_CAP_MS: u64 = 10_000;
const DEFAULT_WORKERS: usize = 2;
const HEALTHZ_BODY: &[u8] = b"ok";
const CONTENT_TYPE_OCTET_STREAM: &str = "application/octet-stream";
const CONTENT_TYPE_TEXT: &str = "text/plain";

type ResponseBody = Full<Bytes>;

struct AppState {
    fixed_payload: Bytes,
    fixed_len: HeaderValue,
    ok_body: Bytes,
    ok_len: HeaderValue,
    sleep_cap_ms: u64,
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
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_io()
        .enable_time()
        .build()?;

    let verbose = std::env::var("RUST_LOG").is_ok();
    if verbose {
        eprintln!(
            "bench-upstream listening on {addr}, fixed_bytes={fixed_bytes}, workers={workers}"
        );
    }

    runtime.block_on(async move {
        let listener = TcpListener::bind(addr).await?;
        run_server(listener, state, verbose).await;
        Ok::<(), std::io::Error>(())
    })?;

    Ok(())
}

#[allow(clippy::collapsible_if)]
async fn run_server(listener: TcpListener, state: Arc<AppState>, verbose: bool) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(err) => {
                if verbose {
                    eprintln!("bench-upstream accept error: {err}");
                }
                continue;
            }
        };

        if let Err(err) = stream.set_nodelay(true) {
            if verbose {
                eprintln!("bench-upstream failed to set TCP_NODELAY: {err}");
            }
            continue;
        }

        let io = TokioIo::new(stream);
        let state = state.clone();
        tokio::spawn(async move {
            let svc = service_fn(move |req| {
                let state = state.clone();
                handle_request(req, state)
            });

            if let Err(err) = http1::Builder::new()
                .keep_alive(true)
                .half_close(false)
                .serve_connection(io, svc)
                .await
            {
                if verbose {
                    eprintln!("bench-upstream connection error: {err}");
                }
            }
        });
    }
}

async fn handle_request<B>(
    req: Request<B>,
    state: Arc<AppState>,
) -> Result<Response<ResponseBody>, Infallible> {
    if req.method() != Method::GET {
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
) -> Response<ResponseBody> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, body_len.clone());

    if close {
        builder = builder.header(header::CONNECTION, "close");
    }

    builder.body(Full::new(body)).unwrap_or_else(|_| {
        let mut fallback = Response::new(Full::new(Bytes::from_static(b"")));
        *fallback.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        fallback
            .headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        fallback
    })
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

fn should_close<B>(req: &Request<B>) -> bool {
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[test]
    fn test_parse_env_functions() {
        unsafe {
            std::env::set_var("TEST_U16", "1234");
        }
        assert_eq!(parse_env_u16("TEST_U16", 0), 1234);
        assert_eq!(parse_env_u16("TEST_MISSING", 5), 5);
        assert_eq!(parse_env_u16("TEST_INVALID", 5), 5); // Assuming it doesn't parse

        unsafe {
            std::env::set_var("TEST_USIZE", "5678");
        }
        assert_eq!(parse_env_usize("TEST_USIZE", 0), 5678);

        unsafe {
            std::env::set_var("TEST_U64", "9012");
        }
        assert_eq!(parse_env_u64("TEST_U64", 0), 9012);
    }

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
            .body(())
            .unwrap();
        assert!(should_close(&req));

        let req = Request::builder()
            .header(CONNECTION, "keep-alive")
            .body(())
            .unwrap();
        assert!(!should_close(&req));
    }

    #[test]
    fn should_close_handles_comma_separated_values() {
        let req = Request::builder()
            .header(CONNECTION, "upgrade, close")
            .body(())
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
        })
    }

    #[tokio::test]
    async fn handle_request_healthz_ok() {
        let state = test_state(8, 10);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/healthz")
            .body(())
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
            .method(Method::GET)
            .uri("/fixed")
            .body(())
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
            .method(Method::GET)
            .uri("/status/999")
            .body(())
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
            .method(Method::GET)
            .uri("/sleep?ms=1")
            .body(())
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
            .method(Method::GET)
            .uri("/sleep")
            .body(())
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
            .method(Method::POST)
            .uri("/fixed")
            .body(())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            resp.headers().get(header::CONTENT_LENGTH).unwrap(),
            &HeaderValue::from_static("6")
        );
    }

    #[tokio::test]
    async fn handle_request_connection_close_echoes_header() {
        let state = test_state(8, 5);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/fixed")
            .header(header::CONNECTION, "close")
            .body(())
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
            .method(Method::GET)
            .uri("/status/503")
            .body(())
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
            .method(Method::GET)
            .uri("/status/abc")
            .body(())
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
            .method(Method::GET)
            .uri("/sleep?ms=50")
            .body(())
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
            .method(Method::GET)
            .uri("/sleep?ms=0")
            .body(())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn handle_request_fixed_content_type_is_octet_stream() {
        let state = test_state(3, 5);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/fixed")
            .body(())
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
            .method(Method::GET)
            .uri("/unknown")
            .header(header::CONNECTION, "close")
            .body(())
            .unwrap();

        let resp = handle_request(req, state).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONNECTION).unwrap(),
            &HeaderValue::from_static("close")
        );
    }

    #[tokio::test]
    async fn keepalive_reuses_connection_without_date_or_server() {
        let state = test_state(4, 5);
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(run_server(listener, state.clone(), false));

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
