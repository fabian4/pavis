use pavis::proxy::service::test_exports::{calculate_path_rewrite, reuse_key_hash};
use pavis_core::{
    ClientCert, ClientCertChain, HeaderPredicates, HeadersPolicy, MethodPredicate, Path, PathMatch,
    Principal, RetryPolicy, Rewrite, RewriteHost, RewritePath, Route, RouteAction, RouteMatcher,
    Timeout,
};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tracing::subscriber;

fn capture_logs<F: FnOnce()>(f: F) -> String {
    #[derive(Clone)]
    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for BufferWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || BufferWriter(writer.clone()))
        .with_ansi(false)
        .without_time()
        .finish();

    subscriber::with_default(subscriber, f);

    String::from_utf8(buffer.lock().unwrap().clone()).unwrap_or_default()
}

fn base_route(path_match: PathMatch) -> Route {
    Route {
        matcher: RouteMatcher {
            path: path_match,
            method: MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/svc".to_string()),
                to: Path("/v1".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Direct {
            status: 200,
            body: "ok".to_string(),
        },
        principal: Principal::Any,
    }
}

#[test]
fn reuse_key_hash_changes_with_cert_paths() {
    let addr: SocketAddr = "10.0.0.1:8080".parse().unwrap();
    let cert_a = ClientCert::Enabled {
        cert_path: pavis_core::Path("/tmp/a.pem".into()),
        key_path: pavis_core::Path("/tmp/a.key".into()),
        chain: ClientCertChain::None,
    };
    let cert_b = ClientCert::Enabled {
        cert_path: pavis_core::Path("/tmp/b.pem".into()),
        key_path: pavis_core::Path("/tmp/b.key".into()),
        chain: ClientCertChain::None,
    };

    let base = reuse_key_hash(&addr, "svc", None, Some(&cert_a));
    let other = reuse_key_hash(&addr, "svc", None, Some(&cert_b));
    assert_ne!(base, other);
}

#[test]
fn reuse_key_hash_is_stable_for_same_inputs() {
    let addr: SocketAddr = "10.0.0.1:8080".parse().unwrap();
    let cert = ClientCert::Enabled {
        cert_path: pavis_core::Path("/tmp/c.pem".into()),
        key_path: pavis_core::Path("/tmp/c.key".into()),
        chain: ClientCertChain::File {
            path: pavis_core::Path("/tmp/chain.pem".into()),
        },
    };

    let first = reuse_key_hash(&addr, "svc", None, Some(&cert));
    let second = reuse_key_hash(&addr, "svc", None, Some(&cert));
    assert_eq!(first, second);
}

#[test]
fn prefix_rewrite_applies_and_keeps_query() {
    let route = base_route(PathMatch::Prefix {
        path: Path("/svc".to_string()),
    });
    let rewritten = calculate_path_rewrite(&route, "/svc/foo", Some("a=1&b=2"))
        .expect("rewrite should succeed");
    assert_eq!(
        rewritten.path_and_query().map(|pq| pq.as_str()),
        Some("/v1/foo?a=1&b=2")
    );
}

#[test]
fn regex_rewrite_is_skipped_with_warning() {
    let route = base_route(PathMatch::Regex {
        path: Path(".*".to_string()),
    });
    let logs = capture_logs(|| {
        assert!(calculate_path_rewrite(&route, "/svc/foo", None).is_none());
    });
    assert!(logs.contains("Skipping path rewrite for regex match"));
}

#[test]
fn unmatched_prefix_emits_warning() {
    let route = base_route(PathMatch::Prefix {
        path: Path("/svc".to_string()),
    });
    let logs = capture_logs(|| {
        assert!(calculate_path_rewrite(&route, "/other", None).is_none());
    });
    assert!(logs.contains("Skipping path rewrite due to unmatched prefix"));
}
