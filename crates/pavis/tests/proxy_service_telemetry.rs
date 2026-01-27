mod common;

use common::*;
use pavis::proxy::service::test_exports::Proxy;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::UpstreamName;
use pingora::prelude::ProxyHttp;
use std::sync::Arc;

#[tokio::test]
async fn logging_handles_disabled_access_log() {
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: Manager::new(&[]).expect("manager"),
        config_version: None,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    proxy.logging(&mut session, None, &mut ctx).await;
}

#[tokio::test]
async fn test_proxy_logging_with_upstream() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
            config_version: None,
        })),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));
    proxy.logging(&mut session, None, &mut ctx).await;
}
