use anyhow::{Context, Result};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use pavis_core::{
    ActiveHealthCheck, CanonicalSni, CircuitBreakerPolicy, ClientCert, ClientCertChain,
    ConsecutiveErrors, Discovery, EndpointAddr, MaxConnections, MaxPendingRequests,
    OutlierDetectionPolicy, Path, ReuseAcrossSni, SniName, TlsVerify, UpstreamBuilder, UpstreamCa,
    UpstreamId, UpstreamName,
};

use super::dto_adapter;
use super::materialize::{
    DEFAULT_POOL_QUEUE_CAPACITY, DEFAULT_POOL_QUEUE_TIMEOUT_MS, default_connection_timeout,
    default_idle_timeout, default_pool_config, duration_to_connect, duration_to_policy,
    duration_to_required, duration_to_tcp_keepalive, materialize_pool_max, materialize_queue_value,
    validate_recv_buffer_size,
};
use crate::config::types::{ClientCertChainMode, SniMode, Upstream as CodecUpstream};

pub fn to_runtime(upstreams: Vec<CodecUpstream>) -> Result<Vec<pavis_core::Upstream>> {
    let mut runtime_upstreams = Vec::new();

    for (index, u) in upstreams.into_iter().enumerate() {
        let discovery = u.discovery.unwrap_or_default();
        let balancer = u.balancer.unwrap_or_default();
        let protocol = u.protocol.unwrap_or_default();
        let pool_config = u.pool.unwrap_or_else(default_pool_config);

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let port = NonZeroU16::new(e.port)
                .ok_or_else(|| anyhow::anyhow!("endpoint port must be > 0"))?;
            let address = match discovery {
                Discovery::Static => {
                    let ip: IpAddr = e.address.parse().with_context(|| {
                        format!(
                            "Invalid endpoint IP '{}' for upstream '{}'",
                            e.address, u.name
                        )
                    })?;
                    EndpointAddr::Ip {
                        address: ip,
                        port: pavis_core::Port(port),
                    }
                }
                Discovery::Logical | Discovery::Strict { .. } => EndpointAddr::Dns {
                    host: pavis_core::Hostname(e.address),
                    port: pavis_core::Port(port),
                },
                _ => return Err(anyhow::anyhow!("unknown discovery variant")),
            };

            let weight = e.weight.unwrap_or(1);
            let weight = u16::try_from(weight).context("endpoint weight exceeds u16::MAX")?;
            let weight = NonZeroU16::new(weight)
                .ok_or_else(|| anyhow::anyhow!("endpoint weight must be > 0"))?;
            endpoints.push(pavis_core::Endpoint {
                address,
                weight: pavis_core::Weight(weight),
            });
        }

        let idle = duration_to_policy(pool_config.idle.unwrap_or_else(default_idle_timeout))?;
        let connect = duration_to_connect(
            pool_config
                .connect
                .unwrap_or_else(default_connection_timeout),
        )?;
        let max = materialize_pool_max(pool_config.max, &u.name, index)?;
        let queue_capacity = materialize_queue_value(
            pool_config.queue_capacity,
            DEFAULT_POOL_QUEUE_CAPACITY,
            "queue_capacity",
            &u.name,
            index,
        )?;
        let queue_timeout_ms = materialize_queue_value(
            pool_config.queue_timeout_ms,
            DEFAULT_POOL_QUEUE_TIMEOUT_MS,
            "queue_timeout_ms",
            &u.name,
            index,
        )?;

        // Process TCP tuning parameters
        let tcp_keepalive = match pool_config.tcp_keepalive {
            Some(duration) => Some(duration_to_tcp_keepalive(duration, &u.name, index)?),
            None => None,
        };

        let tcp_nodelay = pool_config.tcp_nodelay;

        let recv_buffer_size = match pool_config.recv_buffer_size {
            Some(size) => Some(validate_recv_buffer_size(size, &u.name, index)?),
            None => None,
        };

        let pool = pavis_core::Pool {
            idle,
            connect,
            max: pavis_core::ConnectionLimit(max),
            queue: pavis_core::PoolQueue {
                capacity: queue_capacity,
                timeout_ms: queue_timeout_ms,
            },
            tcp_keepalive,
            tcp_nodelay,
            recv_buffer_size,
        };

        let tls = match u.tls {
            None => pavis_core::TlsPolicy::Disabled,
            Some(t) => {
                let enabled = t.enabled.unwrap_or(true);
                if !enabled {
                    pavis_core::TlsPolicy::Disabled
                } else {
                    let verify_cert = t.verify_cert.unwrap_or(true);
                    let verify_hostname = t.verify_hostname.unwrap_or(true);
                    let verify = match (verify_cert, verify_hostname) {
                        (false, _) => TlsVerify::Disabled,
                        (true, false) => TlsVerify::CaOnly,
                        (true, true) => TlsVerify::Full,
                    };
                    let sni = match t.sni_mode {
                        Some(SniMode::Auto) => {
                            if t.sni.is_some() {
                                return Err(anyhow::anyhow!(
                                    "upstream '{}' sets sni_mode=auto but also provides sni",
                                    u.name
                                ));
                            }
                            SniName::Auto
                        }
                        Some(SniMode::Name) => {
                            let name = t.sni.ok_or_else(|| {
                                anyhow::anyhow!(
                                    "upstream '{}' sets sni_mode=name without sni",
                                    u.name
                                )
                            })?;
                            SniName::Name(pavis_core::Hostname(name))
                        }
                        Some(SniMode::Disabled) => {
                            if t.sni.is_some() {
                                return Err(anyhow::anyhow!(
                                    "upstream '{}' sets sni_mode=disabled but also provides sni",
                                    u.name
                                ));
                            }
                            SniName::Disabled
                        }
                        None => match t.sni {
                            Some(name) => SniName::Name(pavis_core::Hostname(name)),
                            None => SniName::Auto,
                        },
                    };
                    if matches!(verify, TlsVerify::Full) && matches!(sni, SniName::Disabled) {
                        return Err(anyhow::anyhow!(
                            "upstream '{}' verify=full requires sni=auto or sni=name",
                            u.name
                        ));
                    }
                    let canonical_sni = match t.canonical_sni {
                        Some(name) => {
                            if name.trim().is_empty() {
                                return Err(anyhow::anyhow!(
                                    "upstream '{}' canonical_sni cannot be empty",
                                    u.name
                                ));
                            }
                            CanonicalSni::Enabled {
                                name: pavis_core::Hostname(name),
                            }
                        }
                        None => CanonicalSni::Disabled,
                    };
                    let reuse_across_sni = match t.reuse_across_sni.unwrap_or(false) {
                        true => ReuseAcrossSni::Enabled,
                        false => ReuseAcrossSni::Disabled,
                    };
                    if matches!(reuse_across_sni, ReuseAcrossSni::Enabled)
                        && matches!(verify, TlsVerify::Disabled)
                    {
                        return Err(anyhow::anyhow!(
                            "upstream '{}' reuse_across_sni requires verify != disabled",
                            u.name
                        ));
                    }
                    let ca = match t.ca_bundle_path {
                        Some(path) => {
                            if path.trim().is_empty() {
                                return Err(anyhow::anyhow!(
                                    "upstream '{}' ca_bundle_path cannot be empty",
                                    u.name
                                ));
                            }
                            UpstreamCa::File { path: Path(path) }
                        }
                        None => UpstreamCa::System,
                    };
                    let cert = match t.cert {
                        None => ClientCert::Disabled,
                        Some(cc) => {
                            if cc.cert_path.trim().is_empty() || cc.key_path.trim().is_empty() {
                                return Err(anyhow::anyhow!(
                                    "upstream '{}' cert_path and key_path must be non-empty",
                                    u.name
                                ));
                            }
                            let chain = match (cc.chain_mode, cc.chain_path) {
                                (None, None) => ClientCertChain::None,
                                (None, Some(path)) => {
                                    if path.trim().is_empty() {
                                        return Err(anyhow::anyhow!(
                                            "upstream '{}' chain_path cannot be empty",
                                            u.name
                                        ));
                                    }
                                    ClientCertChain::File { path: Path(path) }
                                }
                                (Some(ClientCertChainMode::File), Some(path)) => {
                                    if path.trim().is_empty() {
                                        return Err(anyhow::anyhow!(
                                            "upstream '{}' chain_path cannot be empty",
                                            u.name
                                        ));
                                    }
                                    ClientCertChain::File { path: Path(path) }
                                }
                                (Some(ClientCertChainMode::File), None) => {
                                    return Err(anyhow::anyhow!(
                                        "upstream '{}' chain_mode=file requires chain_path",
                                        u.name
                                    ));
                                }
                                (Some(ClientCertChainMode::Embedded), None) => {
                                    ClientCertChain::Embedded
                                }
                                (Some(ClientCertChainMode::None), None) => ClientCertChain::None,
                                (Some(ClientCertChainMode::Embedded), Some(_))
                                | (Some(ClientCertChainMode::None), Some(_)) => {
                                    return Err(anyhow::anyhow!(
                                        "upstream '{}' chain_path is only valid with chain_mode=file",
                                        u.name
                                    ));
                                }
                            };
                            ClientCert::Enabled {
                                cert_path: Path(cc.cert_path),
                                key_path: Path(cc.key_path),
                                chain,
                            }
                        }
                    };
                    pavis_core::TlsPolicy::Enabled {
                        verify,
                        sni,
                        canonical_sni,
                        reuse_across_sni,
                        cert,
                        ca,
                    }
                }
            }
        };
        let id = match u.id {
            Some(id) => {
                NonZeroU16::new(id).ok_or_else(|| anyhow::anyhow!("upstream id must be > 0"))?
            }
            None => NonZeroU16::new((index + 1) as u16)
                .ok_or_else(|| anyhow::anyhow!("upstream id must be > 0"))?,
        };

        let outlier_detection = match u.outlier_detection {
            None => OutlierDetectionPolicy::Disabled,
            Some(outlier) => {
                let errors = u32::try_from(outlier.consecutive_errors)
                    .context("outlier_detection.consecutive_errors exceeds u32::MAX")?;
                let errors = NonZeroU32::new(errors).ok_or_else(|| {
                    anyhow::anyhow!("outlier_detection.consecutive_errors must be > 0")
                })?;
                let eject_duration = duration_to_required(
                    outlier.eject_duration,
                    "outlier_detection.eject_duration",
                )?;
                OutlierDetectionPolicy::Enabled {
                    consecutive_errors: ConsecutiveErrors(errors),
                    eject_duration,
                }
            }
        };

        let circuit_breaker = match u.circuit_breaker {
            None => CircuitBreakerPolicy::Disabled,
            Some(cb) => {
                if cb.max_retries.is_some() {
                    return Err(anyhow::anyhow!(
                        "circuit_breaker.max_retries is not supported"
                    ));
                }
                let max_connections = u32::try_from(cb.max_connections)
                    .context("circuit_breaker.max_connections exceeds u32::MAX")?;
                let max_connections = NonZeroU32::new(max_connections).ok_or_else(|| {
                    anyhow::anyhow!("circuit_breaker.max_connections must be > 0")
                })?;
                let max_pending_requests = u32::try_from(cb.max_pending_requests)
                    .context("circuit_breaker.max_pending_requests exceeds u32::MAX")?;
                let max_pending_requests =
                    NonZeroU32::new(max_pending_requests).ok_or_else(|| {
                        anyhow::anyhow!("circuit_breaker.max_pending_requests must be > 0")
                    })?;
                CircuitBreakerPolicy::Enabled {
                    max_connections: MaxConnections(max_connections),
                    max_pending_requests: MaxPendingRequests(max_pending_requests),
                }
            }
        };

        let health_check = match u.health_check {
            None => ActiveHealthCheck::Disabled,
            Some(hc) => {
                if hc.healthy_threshold != 1 || hc.unhealthy_threshold != 1 {
                    return Err(anyhow::anyhow!(
                        "health_check thresholds are not supported (must be 1)"
                    ));
                }
                if hc.path.trim().is_empty() {
                    return Err(anyhow::anyhow!("health_check.path cannot be empty"));
                }
                let interval = duration_to_required(hc.interval, "health_check.interval")?;
                let timeout = match hc.timeout {
                    Some(timeout) => duration_to_required(timeout, "health_check.timeout")?,
                    None => interval,
                };
                if timeout.0.get() > interval.0.get() {
                    return Err(anyhow::anyhow!(
                        "health_check.timeout must be <= health_check.interval"
                    ));
                }
                ActiveHealthCheck::Enabled {
                    path: Path(hc.path),
                    interval,
                    timeout,
                }
            }
        };

        let mut builder = UpstreamBuilder::new()
            .id(UpstreamId(id))
            .name(UpstreamName(u.name))
            .discovery(discovery)
            .balancer(balancer)
            .protocol(protocol)
            .pool(pool)
            .outlier_detection(outlier_detection)
            .circuit_breaker(circuit_breaker)
            .health_check(health_check)
            .tls(tls);
        for endpoint in endpoints {
            builder = builder.add_endpoint(endpoint);
        }
        runtime_upstreams.push(builder.build().map_err(|err| anyhow::anyhow!(err))?);
    }

    Ok(runtime_upstreams)
}

pub fn from_runtime(upstreams: Vec<pavis_core::Upstream>) -> Result<Vec<CodecUpstream>> {
    dto_adapter::from_runtime(upstreams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{
        CircuitBreaker, ClientCertChainMode, ClientCertConfig, ConnectionPoolConfig, Endpoint,
        HealthCheck, OutlierDetection, SniMode, Upstream as CodecUpstream, UpstreamTlsConfig,
    };
    use pavis_core::{Discovery, HttpVersion, LoadBalancer};

    fn minimal_upstream(name: &str) -> CodecUpstream {
        CodecUpstream {
            id: None,
            name: name.to_string(),
            discovery: Some(Discovery::Static),
            balancer: Some(LoadBalancer::Random),
            protocol: Some(HttpVersion::H1),
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: Some(1),
            }],
            pool: None,
            tls: None,
            outlier_detection: None,
            circuit_breaker: None,
            health_check: None,
        }
    }

    #[test]
    fn test_endpoint_port_zero() {
        let upstream = CodecUpstream {
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 0,
                weight: Some(1),
            }],
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("port must be > 0"));
    }

    #[test]
    fn test_endpoint_invalid_ip_static_discovery() {
        let upstream = CodecUpstream {
            discovery: Some(Discovery::Static),
            endpoints: vec![Endpoint {
                address: "not-an-ip".to_string(),
                port: 8080,
                weight: Some(1),
            }],
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid endpoint IP")
        );
    }

    #[test]
    fn test_endpoint_dns_with_logical_discovery() {
        let upstream = CodecUpstream {
            discovery: Some(Discovery::Logical),
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 8080,
                weight: Some(1),
            }],
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_endpoint_weight_zero() {
        let upstream = CodecUpstream {
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: Some(0),
            }],
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("weight must be > 0")
        );
    }

    #[test]
    fn test_tls_disabled_explicit() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(false),
                verify_cert: None,
                verify_hostname: None,
                sni_mode: None,
                sni: None,
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_sni_auto_with_explicit_sni_conflict() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: Some(SniMode::Auto),
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("sni_mode=auto but also provides sni")
        );
    }

    #[test]
    fn test_tls_sni_name_without_sni() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: Some(SniMode::Name),
                sni: None,
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("sni_mode=name without sni")
        );
    }

    #[test]
    fn test_tls_sni_disabled_with_explicit_sni_conflict() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: Some(SniMode::Disabled),
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("sni_mode=disabled but also provides sni")
        );
    }

    #[test]
    fn test_tls_full_verify_with_disabled_sni_conflict() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: Some(SniMode::Disabled),
                sni: None,
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("verify=full requires sni")
        );
    }

    #[test]
    fn test_tls_canonical_sni_empty() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: Some("   ".to_string()),
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("canonical_sni cannot be empty")
        );
    }

    #[test]
    fn test_tls_reuse_across_sni_without_verify() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(false),
                verify_hostname: Some(false),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: Some(true),
                cert: None,
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("reuse_across_sni requires verify")
        );
    }

    #[test]
    fn test_tls_ca_bundle_empty() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: None,
                ca_bundle_path: Some("  ".to_string()),
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("ca_bundle_path cannot be empty")
        );
    }

    #[test]
    fn test_tls_client_cert_empty_paths() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: Some(ClientCertConfig {
                    cert_path: "  ".to_string(),
                    key_path: "key.pem".to_string(),
                    chain_mode: None,
                    chain_path: None,
                }),
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cert_path and key_path must be non-empty")
        );
    }

    #[test]
    fn test_tls_client_cert_chain_path_empty() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: Some(ClientCertConfig {
                    cert_path: "cert.pem".to_string(),
                    key_path: "key.pem".to_string(),
                    chain_mode: None,
                    chain_path: Some("  ".to_string()),
                }),
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chain_path cannot be empty")
        );
    }

    #[test]
    fn test_tls_client_cert_chain_mode_file_without_path() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: Some(ClientCertConfig {
                    cert_path: "cert.pem".to_string(),
                    key_path: "key.pem".to_string(),
                    chain_mode: Some(ClientCertChainMode::File),
                    chain_path: None,
                }),
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chain_mode=file requires chain_path")
        );
    }

    #[test]
    fn test_tls_client_cert_chain_mode_embedded_with_path() {
        let upstream = CodecUpstream {
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(true),
                sni_mode: None,
                sni: Some("example.com".to_string()),
                canonical_sni: None,
                reuse_across_sni: None,
                cert: Some(ClientCertConfig {
                    cert_path: "cert.pem".to_string(),
                    key_path: "key.pem".to_string(),
                    chain_mode: Some(ClientCertChainMode::Embedded),
                    chain_path: Some("chain.pem".to_string()),
                }),
                ca_bundle_path: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chain_path is only valid with chain_mode=file")
        );
    }

    #[test]
    fn test_outlier_detection_consecutive_errors_zero() {
        let upstream = CodecUpstream {
            outlier_detection: Some(OutlierDetection {
                consecutive_errors: 0,
                eject_duration: std::time::Duration::from_secs(30),
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("consecutive_errors must be > 0")
        );
    }

    #[test]
    fn test_circuit_breaker_max_retries_not_supported() {
        let upstream = CodecUpstream {
            circuit_breaker: Some(CircuitBreaker {
                max_connections: 100,
                max_pending_requests: 50,
                max_retries: Some(3),
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("max_retries is not supported")
        );
    }

    #[test]
    fn test_circuit_breaker_max_connections_zero() {
        let upstream = CodecUpstream {
            circuit_breaker: Some(CircuitBreaker {
                max_connections: 0,
                max_pending_requests: 50,
                max_retries: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("max_connections must be > 0")
        );
    }

    #[test]
    fn test_circuit_breaker_max_pending_requests_zero() {
        let upstream = CodecUpstream {
            circuit_breaker: Some(CircuitBreaker {
                max_connections: 100,
                max_pending_requests: 0,
                max_retries: None,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("max_pending_requests must be > 0")
        );
    }

    #[test]
    fn test_health_check_invalid_thresholds() {
        let upstream = CodecUpstream {
            health_check: Some(HealthCheck {
                path: "/health".to_string(),
                interval: std::time::Duration::from_secs(10),
                timeout: Some(std::time::Duration::from_secs(5)),
                healthy_threshold: 2,
                unhealthy_threshold: 1,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("thresholds are not supported")
        );
    }

    #[test]
    fn test_health_check_empty_path() {
        let upstream = CodecUpstream {
            health_check: Some(HealthCheck {
                path: "  ".to_string(),
                interval: std::time::Duration::from_secs(10),
                timeout: Some(std::time::Duration::from_secs(5)),
                healthy_threshold: 1,
                unhealthy_threshold: 1,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("path cannot be empty")
        );
    }

    #[test]
    fn test_health_check_timeout_exceeds_interval() {
        let upstream = CodecUpstream {
            health_check: Some(HealthCheck {
                path: "/health".to_string(),
                interval: std::time::Duration::from_secs(5),
                timeout: Some(std::time::Duration::from_secs(10)),
                healthy_threshold: 1,
                unhealthy_threshold: 1,
            }),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("timeout must be <= health_check.interval")
        );
    }

    #[test]
    fn test_upstream_id_zero() {
        let upstream = CodecUpstream {
            id: Some(0),
            ..minimal_upstream("test")
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("id must be > 0"));
    }

    #[test]
    fn test_successful_minimal_upstream() {
        let upstream = minimal_upstream("test");
        let result = to_runtime(vec![upstream]);
        assert!(result.is_ok());
        let upstreams = result.unwrap();
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0].name.0, "test");
    }

    #[test]
    fn test_successful_upstream_with_all_features() {
        let upstream = CodecUpstream {
            id: Some(42),
            name: "full-featured".to_string(),
            discovery: Some(Discovery::Static),
            balancer: Some(LoadBalancer::RoundRobin),
            protocol: Some(HttpVersion::H2),
            endpoints: vec![
                Endpoint {
                    address: "127.0.0.1".to_string(),
                    port: 8080,
                    weight: Some(10),
                },
                Endpoint {
                    address: "127.0.0.2".to_string(),
                    port: 8081,
                    weight: Some(5),
                },
            ],
            pool: Some(ConnectionPoolConfig {
                idle: Some(std::time::Duration::from_secs(60)),
                connect: Some(std::time::Duration::from_secs(5)),
                max: Some(100),
                queue_capacity: Some(50),
                queue_timeout_ms: Some(1000),
                tcp_keepalive: Some(std::time::Duration::from_secs(30)),
                tcp_nodelay: Some(true),
                recv_buffer_size: Some(65536),
            }),
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(true),
                verify_hostname: Some(false),
                sni_mode: Some(SniMode::Name),
                sni: Some("backend.example.com".to_string()),
                canonical_sni: Some("canonical.example.com".to_string()),
                reuse_across_sni: Some(true),
                cert: Some(ClientCertConfig {
                    cert_path: "/path/to/cert.pem".to_string(),
                    key_path: "/path/to/key.pem".to_string(),
                    chain_mode: Some(ClientCertChainMode::Embedded),
                    chain_path: None,
                }),
                ca_bundle_path: Some("/path/to/ca.pem".to_string()),
            }),
            outlier_detection: Some(OutlierDetection {
                consecutive_errors: 5,
                eject_duration: std::time::Duration::from_secs(30),
            }),
            circuit_breaker: Some(CircuitBreaker {
                max_connections: 1000,
                max_pending_requests: 500,
                max_retries: None,
            }),
            health_check: Some(HealthCheck {
                path: "/health".to_string(),
                interval: std::time::Duration::from_secs(10),
                timeout: Some(std::time::Duration::from_secs(5)),
                healthy_threshold: 1,
                unhealthy_threshold: 1,
            }),
        };

        let result = to_runtime(vec![upstream]);
        assert!(result.is_ok());
        let upstreams = result.unwrap();
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0].name.0, "full-featured");
        assert_eq!(upstreams[0].id.0.get(), 42);
    }
}
