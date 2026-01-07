use anyhow::Result;
use pavis_core::{
    ClientAuth, Destination, HeadersPolicy, Path, Path as RoutePath, PathMatch, Principal,
    RetryPolicy, Rewrite, RewriteHost, RewritePath, Route, RouteAction, Timeout, TlsConfig,
    UpstreamName, VirtualHost, Weight,
};
use std::fs;
use std::num::NonZeroU16;
use std::process::Command;

use super::support::{pavis_target, publish, relay_env, runtime_config, upstreams};

// TODO: This test requires proper client certificate support to verify RBAC
// For now, we test the configuration and ensure routes with Principal::Prefix are accepted
// Full namespace authorization testing would require client cert with SPIFFE IDs
#[tokio::test]
#[ignore = "Requires client certificate support for full RBAC testing"]
async fn integrated_namespace_authorization() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    let is_docker = std::env::var("TEST_MODE").unwrap_or_default() == "docker";

    // Setup certificate directory
    let tmp_dir = std::env::temp_dir().join("pavis_integrated_ns_authz");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir)?;

    let server_cert = tmp_dir.join("server_cert.pem");
    let server_key = tmp_dir.join("server_key.pem");
    let ca_cert = tmp_dir.join("ca_cert.pem");
    let ca_key = tmp_dir.join("ca_key.pem");

    // 1. Generate CA certificate
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            ca_key.to_str().unwrap(),
            "-out",
            ca_cert.to_str().unwrap(),
            "-subj",
            "/CN=Test CA",
            "-days",
            "1",
        ])
        .status()?;
    assert!(status.success(), "Failed to generate CA certificate");

    // 2. Generate server certificate
    let status = Command::new("openssl")
        .args([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            server_key.to_str().unwrap(),
            "-out",
            tmp_dir.join("server.csr").to_str().unwrap(),
            "-subj",
            "/CN=localhost",
        ])
        .status()?;
    assert!(status.success(), "Failed to generate server CSR");

    let status = Command::new("openssl")
        .args([
            "x509",
            "-req",
            "-in",
            tmp_dir.join("server.csr").to_str().unwrap(),
            "-CA",
            ca_cert.to_str().unwrap(),
            "-CAkey",
            ca_key.to_str().unwrap(),
            "-CAcreateserial",
            "-out",
            server_cert.to_str().unwrap(),
            "-days",
            "1",
        ])
        .status()?;
    assert!(status.success(), "Failed to sign server certificate");

    // 3. Configure Pavis with namespace-level authorization
    let mut config = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );

    let (server_cert_path, server_key_path, ca_cert_path) = if is_docker {
        (
            "/pavis/certs/server_cert.pem".to_string(),
            "/pavis/certs/server_key.pem".to_string(),
            "/pavis/certs/ca_cert.pem".to_string(),
        )
    } else {
        (
            server_cert.to_str().unwrap().to_string(),
            server_key.to_str().unwrap().to_string(),
            ca_cert.to_str().unwrap().to_string(),
        )
    };

    config.listeners[0].tls = TlsConfig::Enabled {
        cert_path: RoutePath(server_cert_path),
        key_path: RoutePath(server_key_path),
        client_auth: ClientAuth::Required {
            ca_path: RoutePath(ca_cert_path),
        },
    };

    // Create routes with namespace-level authorization
    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![
            // Route restricted to prod namespace
            Route {
                matcher: PathMatch::Prefix {
                    path: Path("/prod".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: Principal::Prefix {
                    prefix: "spiffe://cluster.local/ns/prod/".to_string(),
                },
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("upstream-a".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            // Fallback route
            Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("upstream-b".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    }];

    // 4. Publish configuration (validates schema)
    let publish_resp = publish(relay.base_url(), 1, &config).await?;
    assert!(
        publish_resp.status().is_success(),
        "Configuration with Principal::Prefix should be accepted by relay"
    );

    // TODO: Add actual authorization tests with client certificates
    // This would require:
    // 1. Generate client cert with SPIFFE ID in SAN (spiffe://cluster.local/ns/prod/sa/app-a)
    // 2. Generate dev client cert (spiffe://cluster.local/ns/dev/sa/app-b)
    // 3. Use curl or native-tls to make requests with certificates
    // 4. Verify prod client gets 200, dev client gets 403

    Ok(())
}
