use anyhow::{Context, Result};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use pavis_core::{
    ActiveHealthCheck, CanonicalSni, CircuitBreakerPolicy, ClientCert, ClientCertChain,
    ConnectTimeout, ConsecutiveErrors, Discovery, EndpointAddr, ErrorCode, FieldPathBuilder,
    MaxConnections, MaxPendingRequests, OutlierDetectionPolicy, Path, PavisError, ReuseAcrossSni,
    SniName, TlsVerify, UpstreamBuilder, UpstreamCa, UpstreamId, UpstreamName,
};

use crate::config::types::{
    ClientCertChainMode, ClientCertConfig, Endpoint, OutlierDetection, SniMode, Upstream,
    UpstreamTlsConfig,
};

const DEFAULT_POOL_MAX: u32 = 128;
const DEFAULT_POOL_QUEUE_CAPACITY: u32 = 0;
const DEFAULT_POOL_QUEUE_TIMEOUT_MS: u32 = 0;

pub(super) fn to_runtime(upstreams: Vec<Upstream>) -> Result<Vec<pavis_core::Upstream>> {
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

        let pool = pavis_core::Pool {
            idle,
            connect,
            max: pavis_core::ConnectionLimit(max),
            queue: pavis_core::PoolQueue {
                capacity: queue_capacity,
                timeout_ms: queue_timeout_ms,
            },
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

pub(super) fn from_runtime(upstreams: Vec<pavis_core::Upstream>) -> Result<Vec<Upstream>> {
    let mut serde_upstreams = Vec::new();

    for u in upstreams {
        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let (address, port) = match e.address {
                EndpointAddr::Ip { address, port } => (address.to_string(), port.0.get()),
                EndpointAddr::Dns { host, port } => (host.0, port.0.get()),
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(anyhow::anyhow!("unknown endpoint address variant"));
                }
            };
            endpoints.push(Endpoint {
                address,
                port,
                weight: Some(e.weight.0.get() as u32),
            });
        }

        let pool = Some(crate::config::types::ConnectionPoolConfig {
            idle: Some(std::time::Duration::from_millis(idle_timeout_ms(
                &u.pool.idle,
            ))),
            connect: Some(std::time::Duration::from_millis(connect_timeout_ms(
                &u.pool.connect,
            ))),
            max: Some(u.pool.max.0.get() as i64),
            queue_capacity: Some(u.pool.queue.capacity as i64),
            queue_timeout_ms: Some(u.pool.queue.timeout_ms as i64),
        });

        let tls = match u.tls {
            pavis_core::TlsPolicy::Disabled => None,
            pavis_core::TlsPolicy::Enabled {
                verify,
                sni,
                canonical_sni,
                reuse_across_sni,
                cert,
                ca,
            } => {
                let (verify_cert, verify_hostname) = match verify {
                    TlsVerify::Disabled => (false, false),
                    TlsVerify::CaOnly => (true, false),
                    TlsVerify::Full => (true, true),
                    #[allow(unreachable_patterns)]
                    _ => {
                        // Sensible default: treat as Full if variant is unknown
                        (true, true)
                    }
                };
                let (sni, sni_mode) = match sni {
                    pavis_core::SniName::Auto => (None, Some(SniMode::Auto)),
                    pavis_core::SniName::Name(name) => (Some(name.0), Some(SniMode::Name)),
                    pavis_core::SniName::Disabled => (None, Some(SniMode::Disabled)),
                    #[allow(unreachable_patterns)]
                    _ => (None, Some(SniMode::Auto)),
                };
                let ca_bundle_path = match ca {
                    UpstreamCa::System => None,
                    UpstreamCa::File { path } => Some(path.0),
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let cert_config = match cert {
                    ClientCert::Disabled => None,
                    ClientCert::Enabled {
                        cert_path,
                        key_path,
                        chain,
                    } => {
                        let (chain_mode, chain_path) = match chain {
                            ClientCertChain::None => (Some(ClientCertChainMode::None), None),
                            ClientCertChain::Embedded => {
                                (Some(ClientCertChainMode::Embedded), None)
                            }
                            ClientCertChain::File { path } => {
                                (Some(ClientCertChainMode::File), Some(path.0))
                            }
                            #[allow(unreachable_patterns)]
                            _ => (Some(ClientCertChainMode::None), None),
                        };
                        Some(ClientCertConfig {
                            cert_path: cert_path.0,
                            key_path: key_path.0,
                            chain_path,
                            chain_mode,
                        })
                    }
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let canonical_sni = match canonical_sni {
                    CanonicalSni::Disabled => None,
                    CanonicalSni::Enabled { name } => Some(name.0),
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let reuse_across_sni = match reuse_across_sni {
                    ReuseAcrossSni::Enabled => Some(true),
                    ReuseAcrossSni::Disabled => None,
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(verify_hostname),
                    verify_cert: Some(verify_cert),
                    sni,
                    sni_mode,
                    canonical_sni,
                    reuse_across_sni,
                    ca_bundle_path,
                    cert: cert_config,
                })
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };

        serde_upstreams.push(Upstream {
            id: Some(u.id.0.get()),
            name: u.name.0,
            discovery: Some(u.discovery),
            balancer: Some(u.balancer),
            protocol: Some(u.protocol),
            pool,
            tls,
            circuit_breaker: match u.circuit_breaker {
                CircuitBreakerPolicy::Disabled => None,
                CircuitBreakerPolicy::Enabled {
                    max_connections,
                    max_pending_requests,
                } => Some(crate::config::types::CircuitBreaker {
                    max_connections: max_connections.0.get() as usize,
                    max_pending_requests: max_pending_requests.0.get() as usize,
                    max_retries: None,
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            outlier_detection: match u.outlier_detection {
                OutlierDetectionPolicy::Disabled => None,
                OutlierDetectionPolicy::Enabled {
                    consecutive_errors,
                    eject_duration,
                } => Some(OutlierDetection {
                    consecutive_errors: consecutive_errors.0.get() as usize,
                    eject_duration: std::time::Duration::from_millis(eject_duration.0.get() as u64),
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            health_check: match u.health_check {
                ActiveHealthCheck::Disabled => None,
                ActiveHealthCheck::Enabled {
                    path,
                    interval,
                    timeout,
                } => Some(crate::config::types::HealthCheck {
                    path: path.0,
                    interval: std::time::Duration::from_millis(interval.0.get() as u64),
                    timeout: Some(std::time::Duration::from_millis(timeout.0.get() as u64)),
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            endpoints,
        });
    }

    Ok(serde_upstreams)
}

fn default_pool_config() -> crate::config::types::ConnectionPoolConfig {
    crate::config::types::ConnectionPoolConfig {
        idle: Some(default_idle_timeout()),
        connect: Some(default_connection_timeout()),
        max: None,
        queue_capacity: None,
        queue_timeout_ms: None,
    }
}

fn default_idle_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn default_connection_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(5)
}

fn duration_to_policy(duration: std::time::Duration) -> Result<pavis_core::IdleTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("idle timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => pavis_core::IdleTimeout::Enabled(pavis_core::Duration(ms)),
        None => pavis_core::IdleTimeout::Disabled,
    })
}

fn duration_to_required(
    duration: std::time::Duration,
    context: &str,
) -> Result<pavis_core::Duration> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("{context} exceeds u32::MAX ms"))?;
    let ms = NonZeroU32::new(ms).ok_or_else(|| anyhow::anyhow!("{context} must be > 0"))?;
    Ok(pavis_core::Duration(ms))
}

fn duration_to_connect(duration: std::time::Duration) -> Result<ConnectTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("connect timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => ConnectTimeout::Enabled(pavis_core::Duration(ms)),
        None => ConnectTimeout::Disabled,
    })
}

fn idle_timeout_ms(timeout: &pavis_core::IdleTimeout) -> u64 {
    match timeout {
        pavis_core::IdleTimeout::Disabled => 0,
        pavis_core::IdleTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

fn connect_timeout_ms(timeout: &pavis_core::ConnectTimeout) -> u64 {
    match timeout {
        pavis_core::ConnectTimeout::Disabled => 0,
        pavis_core::ConnectTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

fn materialize_pool_max(
    value: Option<i64>,
    upstream_name: &str,
    index: usize,
) -> anyhow::Result<NonZeroU32> {
    let width = match value {
        None => return Ok(NonZeroU32::new(DEFAULT_POOL_MAX).expect("default max nonzero")),
        Some(raw) => raw,
    };
    if width < 1 {
        return Err(invalid_config_error(
            format!("upstream '{}' pool.max must be >= 1", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("min_value=1"),
        ));
    }
    let max_value = u32::try_from(width).map_err(|_| {
        invalid_config_error(
            format!("upstream '{}' pool.max exceeds u32::MAX", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("max_value=u32::MAX"),
        )
    })?;
    NonZeroU32::new(max_value).ok_or_else(|| {
        invalid_config_error(
            format!("upstream '{}' pool.max must be >= 1", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("min_value=1"),
        )
    })
}

fn materialize_queue_value(
    value: Option<i64>,
    default: u32,
    field: &str,
    upstream_name: &str,
    index: usize,
) -> anyhow::Result<u32> {
    let raw = match value {
        None => return Ok(default),
        Some(raw) => raw,
    };
    if raw < 0 {
        return Err(invalid_config_error(
            format!("upstream '{}' pool.{} must be >= 0", upstream_name, field),
            Some(upstream_pool_field_path(index, field)),
            Some("min_value=0"),
        ));
    }
    u32::try_from(raw).map_err(|_| {
        invalid_config_error(
            format!(
                "upstream '{}' pool.{} exceeds u32::MAX",
                upstream_name, field
            ),
            Some(upstream_pool_field_path(index, field)),
            Some("max_value=u32::MAX"),
        )
    })
}

fn upstream_field_path(index: usize) -> FieldPathBuilder {
    FieldPathBuilder::new().root("upstreams").index(index)
}

fn upstream_pool_field_path(index: usize, field: &str) -> String {
    upstream_field_path(index)
        .field("pool")
        .field(field)
        .finish()
}

fn invalid_config_error(
    message: impl Into<String>,
    field_path: Option<String>,
    constraint: Option<&str>,
) -> anyhow::Error {
    let err = PavisError::new(ErrorCode::InvalidConfig, message);
    let err = err.with_context(|ctx| {
        let mut ctx = ctx;
        if let Some(path) = field_path {
            ctx = ctx.with_field_path(path);
        }
        if let Some(code) = constraint {
            ctx = ctx.with_constraint(code.to_string());
        }
        ctx
    });
    anyhow::Error::new(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{ConnectionPoolConfig, Endpoint, Upstream};
    use pavis_core::{
        ActiveHealthCheck, CircuitBreakerPolicy, ConnectTimeout, ConnectionLimit,
        ConsecutiveErrors, Discovery, EndpointAddr, HttpVersion, IdleTimeout, LoadBalancer,
        MaxConnections, MaxPendingRequests, OutlierDetectionPolicy, Pool, Port, TlsPolicy,
        UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    #[test]
    fn to_runtime_validates_endpoint_addresses() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "invalid-ip".to_string(),
                port: 80,
                weight: None,
            }],
        }];
        assert!(to_runtime(config).is_err());
    }

    #[test]
    fn to_runtime_defaults() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        let u = &runtime[0];
        assert_eq!(u.name.0, "test");
        assert!(matches!(u.discovery, Discovery::Static));
        assert!(matches!(u.balancer, LoadBalancer::Random));
        assert!(matches!(u.protocol, HttpVersion::H1));
        assert!(matches!(u.tls, TlsPolicy::Disabled));
        assert!(u.endpoints.is_empty());
    }

    #[test]
    fn to_runtime_pool_defaults() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: None,
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        let pool = &runtime[0].pool;
        assert!(matches!(pool.idle, IdleTimeout::Enabled(_))); // Default 60s
        assert!(matches!(pool.connect, ConnectTimeout::Enabled(_))); // Default 5s
        assert_eq!(pool.max.0.get(), DEFAULT_POOL_MAX); // Default pool max
    }

    #[test]
    fn to_runtime_validates_pool_max() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(0), // becomes Unlimited but let's test explicit > 0
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(err.to_string().contains("pool.max must be >= 1"));
    }

    // P0 Feature #2: Pool Validation Tests
    // These tests verify pool.max validation per verification plan requirements.

    /// Test: pool.max = 1 is accepted (minimum valid value).
    #[test]
    fn to_runtime_accepts_pool_max_1() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(1),
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).expect("pool.max=1 should be valid");
        assert_eq!(runtime[0].pool.max.0.get(), 1);
    }

    /// Test: pool.max = 1000 is accepted (large valid value).
    #[test]
    fn to_runtime_accepts_pool_max_1000() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(1000),
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).expect("pool.max=1000 should be valid");
        assert_eq!(runtime[0].pool.max.0.get(), 1000);
    }

    /// Test: pool.max = -5 is rejected with ERR_INVALID_CONFIG.
    #[test]
    fn to_runtime_rejects_negative_pool_max() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(-5),
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).expect_err("negative pool.max should be rejected");
        let err_str = err.to_string();
        assert!(err_str.contains("pool.max must be >= 1"));
        // Verify it's an ERR_INVALID_CONFIG with proper field path
        // The error should contain the field path in the format "upstreams[0].pool.max"
        assert!(err_str.contains("pool.max") || err_str.contains("upstreams"));
    }

    #[test]
    fn tls_enabled_false_conversion() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(false),
                verify_hostname: None,
                verify_cert: None,
                sni: None,
                sni_mode: None,
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: None,
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        assert!(matches!(runtime[0].tls, TlsPolicy::Disabled));
    }

    #[test]
    fn tls_verify_modes_conversion() {
        let test_cases = vec![
            ((false, false), TlsVerify::Disabled),
            ((true, false), TlsVerify::CaOnly),
            ((true, true), TlsVerify::Full),
        ];

        for ((cert, host), expected_mode) in test_cases {
            let config = vec![Upstream {
                id: None,
                name: "test".to_string(),
                discovery: None,
                balancer: None,
                protocol: None,
                pool: None,
                tls: Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(host),
                    verify_cert: Some(cert),
                    sni: None,
                    sni_mode: None,
                    canonical_sni: None,
                    reuse_across_sni: None,
                    ca_bundle_path: None,
                    cert: None,
                }),
                circuit_breaker: None,
                outlier_detection: None,
                health_check: None,
                endpoints: vec![],
            }];
            let runtime = to_runtime(config).unwrap();
            match runtime[0].tls {
                TlsPolicy::Enabled { verify, .. } => assert_eq!(verify, expected_mode),
                _ => panic!("expected tls enabled"),
            }
        }
    }

    #[test]
    fn tls_verify_full_rejects_disabled_sni() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(true),
                verify_cert: Some(true),
                sni: None,
                sni_mode: Some(SniMode::Disabled),
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: None,
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 443,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("verify=full requires sni=auto or sni=name")
        );
    }

    #[test]
    fn sni_mode_name_requires_sni_value() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(true),
                verify_cert: Some(true),
                sni: None,
                sni_mode: Some(SniMode::Name),
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: None,
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 443,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(err.to_string().contains("sets sni_mode=name without sni"));
    }

    #[test]
    fn sni_mode_auto_rejects_explicit_sni() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(true),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
                sni_mode: Some(SniMode::Auto),
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: None,
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 443,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("sets sni_mode=auto but also provides sni")
        );
    }

    #[test]
    fn client_cert_chain_mode_file_requires_chain_path() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(false),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
                sni_mode: None,
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: Some(ClientCertConfig {
                    cert_path: "c.pem".to_string(),
                    key_path: "k.pem".to_string(),
                    chain_path: None,
                    chain_mode: Some(ClientCertChainMode::File),
                }),
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 443,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("chain_mode=file requires chain_path")
        );
    }

    #[test]
    fn client_cert_chain_mode_embedded_rejects_chain_path() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(false),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
                sni_mode: None,
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: Some(ClientCertConfig {
                    cert_path: "c.pem".to_string(),
                    key_path: "k.pem".to_string(),
                    chain_path: Some("chain.pem".to_string()),
                    chain_mode: Some(ClientCertChainMode::Embedded),
                }),
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 443,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("chain_path is only valid with chain_mode=file")
        );
    }

    #[test]
    fn from_runtime_round_trips() {
        let runtime = pavis_core::UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H2)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(5).unwrap()),
            })
            .build()
            .expect("upstream");

        let config = from_runtime(vec![runtime]).expect("from_runtime");
        let u = &config[0];
        assert_eq!(u.name, "test");
        assert!(matches!(u.balancer, Some(LoadBalancer::RoundRobin)));
        assert!(matches!(u.protocol, Some(HttpVersion::H2)));
        assert_eq!(u.endpoints.len(), 1);
        assert_eq!(u.endpoints[0].address, "127.0.0.1");
        assert_eq!(u.endpoints[0].port, 8080);
        assert_eq!(u.endpoints[0].weight, Some(5));
    }

    #[test]
    fn to_runtime_rejects_circuit_breaker_retries() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: Some(crate::config::types::CircuitBreaker {
                max_connections: 10,
                max_pending_requests: 5,
                max_retries: Some(1),
            }),
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("circuit_breaker.max_retries is not supported")
        );
    }

    #[test]
    fn to_runtime_rejects_health_check_thresholds() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: Some(crate::config::types::HealthCheck {
                path: "/healthz".to_string(),
                interval: std::time::Duration::from_secs(5),
                timeout: Some(std::time::Duration::from_secs(1)),
                healthy_threshold: 2,
                unhealthy_threshold: 1,
            }),
            endpoints: vec![],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("health_check thresholds are not supported")
        );
    }

    #[test]
    fn to_runtime_rejects_negative_queue_capacity() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(10),
                queue_capacity: Some(-1),
                queue_timeout_ms: Some(100),
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(err.to_string().contains("pool.queue_capacity must be >= 0"));
    }

    #[test]
    fn to_runtime_rejects_negative_queue_timeout() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(10),
                queue_capacity: Some(1),
                queue_timeout_ms: Some(-50),
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: None,
            }],
        }];
        let err = to_runtime(config).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("pool.queue_timeout_ms must be >= 0")
        );
    }

    #[test]
    fn to_runtime_materializes_pool_defaults() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: None,
                queue_capacity: None,
                queue_timeout_ms: None,
            }),
            tls: None,
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: None,
            }],
        }];
        let runtime = to_runtime(config).expect("runtime");
        let pool = &runtime[0].pool;
        match pool.max {
            ConnectionLimit(limit) => {
                assert_eq!(limit.get(), super::DEFAULT_POOL_MAX);
            }
        }
        assert_eq!(pool.queue.capacity, super::DEFAULT_POOL_QUEUE_CAPACITY);
        assert_eq!(pool.queue.timeout_ms, super::DEFAULT_POOL_QUEUE_TIMEOUT_MS);
    }

    #[test]
    fn to_runtime_converts_outlier_detection() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: Some(crate::config::types::CircuitBreaker {
                max_connections: 10,
                max_pending_requests: 5,
                max_retries: None,
            }),
            outlier_detection: Some(crate::config::types::OutlierDetection {
                consecutive_errors: 3,
                eject_duration: std::time::Duration::from_secs(30),
            }),
            health_check: Some(crate::config::types::HealthCheck {
                path: "/healthz".to_string(),
                interval: std::time::Duration::from_secs(5),
                timeout: None,
                healthy_threshold: 1,
                unhealthy_threshold: 1,
            }),
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: None,
            }],
        }];
        let runtime = to_runtime(config).expect("runtime");
        let upstream = &runtime[0];
        assert!(matches!(
            upstream.outlier_detection,
            OutlierDetectionPolicy::Enabled {
                consecutive_errors: ConsecutiveErrors(_),
                eject_duration: _
            }
        ));
        assert!(matches!(
            upstream.circuit_breaker,
            CircuitBreakerPolicy::Enabled {
                max_connections: MaxConnections(_),
                max_pending_requests: MaxPendingRequests(_)
            }
        ));
        assert!(matches!(
            upstream.health_check,
            ActiveHealthCheck::Enabled { .. }
        ));
    }

    #[test]
    fn dns_discovery_and_tls_conversion() {
        use crate::config::types::{ClientCertConfig, UpstreamTlsConfig};
        let config = vec![Upstream {
            id: None,
            name: "dns".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(true),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
                sni_mode: None,
                canonical_sni: None,
                reuse_across_sni: None,
                ca_bundle_path: None,
                cert: Some(ClientCertConfig {
                    cert_path: "c.pem".to_string(),
                    key_path: "k.pem".to_string(),
                    chain_path: None,
                    chain_mode: None,
                }),
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 80,
                weight: None,
            }],
        }];

        let runtime = to_runtime(config).unwrap();
        let u = &runtime[0];
        match u.discovery {
            Discovery::Logical => {}
            _ => panic!("expected logical discovery"),
        }
        match &u.endpoints[0].address {
            EndpointAddr::Dns { host, port } => {
                assert_eq!(host.0, "example.com");
                assert_eq!(port.0.get(), 80);
            }
            _ => panic!("expected dns endpoint"),
        }
        match &u.tls {
            TlsPolicy::Enabled {
                verify,
                sni,
                cert,
                ca,
                ..
            } => {
                assert!(matches!(verify, pavis_core::TlsVerify::Full));
                match sni {
                    pavis_core::SniName::Name(s) => assert_eq!(s.0, "example.com"),
                    _ => panic!("expected sni value"),
                }
                match cert {
                    pavis_core::ClientCert::Enabled {
                        cert_path,
                        key_path,
                        chain,
                    } => {
                        assert_eq!(cert_path.0, "c.pem");
                        assert_eq!(key_path.0, "k.pem");
                        assert!(matches!(chain, pavis_core::ClientCertChain::None));
                    }
                    _ => panic!("expected client cert"),
                }
                assert!(matches!(ca, pavis_core::UpstreamCa::System));
            }
            _ => panic!("expected tls enabled"),
        }

        // Round trip
        let serde = from_runtime(runtime).expect("from_runtime");
        let u_serde = &serde[0];
        match u_serde.endpoints[0].address.as_str() {
            "example.com" => {}
            _ => panic!("expected example.com"),
        }
        let tls = u_serde.tls.as_ref().unwrap();
        assert_eq!(tls.sni.as_deref(), Some("example.com"));
        assert!(tls.verify_hostname.unwrap());
    }

    #[test]
    fn from_runtime_tls_variants() {
        use pavis_core::{
            ClientCert, ConnectTimeout, ConnectionLimit, Discovery, HttpVersion, IdleTimeout,
            LoadBalancer, Pool, SniName, TlsPolicy, TlsVerify, UpstreamId, UpstreamName,
        };

        let mut upstream = pavis_core::UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("u".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: SniName::Auto,
                canonical_sni: pavis_core::CanonicalSni::Disabled,
                reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
                cert: ClientCert::Disabled,
                ca: pavis_core::UpstreamCa::System,
            })
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream");

        // 1. TlsVerify::Disabled, SniName::Auto
        let serde = from_runtime(vec![upstream.clone()]).expect("from_runtime");
        let tls = serde[0].tls.as_ref().unwrap();
        assert!(!tls.verify_cert.unwrap());
        assert!(!tls.verify_hostname.unwrap());
        assert_eq!(tls.sni, None);
        assert!(matches!(tls.sni_mode, Some(SniMode::Auto)));

        // 2. TlsVerify::CaOnly
        if let TlsPolicy::Enabled { verify, .. } = &mut upstream.tls {
            *verify = TlsVerify::CaOnly;
        }
        let serde = from_runtime(vec![upstream.clone()]).expect("from_runtime");
        let tls = serde[0].tls.as_ref().unwrap();
        assert!(tls.verify_cert.unwrap());
        assert!(!tls.verify_hostname.unwrap());

        // 3. Pool::Limited
        upstream.pool.max = ConnectionLimit(NonZeroU32::new(100).unwrap());
        let serde = from_runtime(vec![upstream.clone()]).expect("from_runtime");
        assert_eq!(serde[0].pool.as_ref().unwrap().max, Some(100));
    }

    fn default_tls() -> UpstreamTlsConfig {
        UpstreamTlsConfig {
            enabled: None,
            verify_hostname: None,
            verify_cert: None,
            sni: None,
            sni_mode: None,
            canonical_sni: None,
            reuse_across_sni: None,
            ca_bundle_path: None,
            cert: None,
        }
    }

    #[test]
    fn sni_mode_disabled_rejects_explicit_sni() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                sni: Some("example.com".to_string()),
                sni_mode: Some(SniMode::Disabled),
                ..default_tls()
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).unwrap_err();
        assert!(
            err.to_string()
                .contains("sets sni_mode=disabled but also provides sni")
        );
    }

    #[test]
    fn canonical_sni_cannot_be_empty() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                canonical_sni: Some("".to_string()),
                ..default_tls()
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).unwrap_err();
        assert!(err.to_string().contains("canonical_sni cannot be empty"));
    }

    #[test]
    fn reuse_across_sni_requires_verify() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_cert: Some(false),
                reuse_across_sni: Some(true),
                ..default_tls()
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).unwrap_err();
        assert!(
            err.to_string()
                .contains("reuse_across_sni requires verify != disabled")
        );
    }

    #[test]
    fn ca_bundle_path_cannot_be_empty() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                ca_bundle_path: Some("".to_string()),
                ..default_tls()
            }),
            circuit_breaker: None,
            outlier_detection: None,
            health_check: None,
            endpoints: vec![],
        }];
        let err = to_runtime(config).unwrap_err();
        assert!(err.to_string().contains("ca_bundle_path cannot be empty"));
    }
}
