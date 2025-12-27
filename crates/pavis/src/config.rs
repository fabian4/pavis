use anyhow::{Context, Result, anyhow};
use memmap2::Mmap;
use pavis_core::config::{Config, ValidatedConfig};
use std::fs::File;

/// Loads and validates configuration from a file.
/// Only supports .pvs (binary) format.
pub fn load_file(path: &str) -> Result<ValidatedConfig> {
    if !path.ends_with(".pvs") {
        return Err(anyhow!(
            "Only .pvs configuration files are supported. Path: {}",
            path
        ));
    }
    let config = load_pvs(path)?;

    config.validate().context("Config validation failed")
}

fn load_pvs(path: &str) -> Result<Config> {
    let file = File::open(path).context("Failed to open .pvs config file")?;
    let mmap = unsafe { Mmap::map(&file).context("Failed to mmap .pvs file")? };

    if mmap.len() < 8 {
        return Err(anyhow!("Config file too small"));
    }

    let magic = &mmap[0..4];
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());

    if magic != pavis_core::PAVIS_MAGIC {
        return Err(anyhow!("Invalid magic bytes in .pvs file. Expected 'PAVS'"));
    }

    if version != pavis_core::PAVIS_VERSION {
        return Err(anyhow!(
            "Version mismatch! File: {}, Proxy: {}. Please recompile config.",
            version,
            pavis_core::PAVIS_VERSION
        ));
    }

    let payload = &mmap[8..];
    let binary_config = pavis_core::deserialize_pvs(payload)?;
    Ok(convert_binary_to_config(binary_config))
}

fn convert_binary_to_config(binary: pavis_core::ProxyConfig) -> Config {
    use pavis_core::config as c;
    use pavis_core::{LoadBalancer, MatchType};

    let mut upstreams = Vec::new();
    for u in binary.upstreams {
        let lb = match u.load_balancer {
            LoadBalancer::Random => c::LoadBalancer::Random,
            LoadBalancer::RoundRobin => c::LoadBalancer::RoundRobin,
        };

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            endpoints.push(c::Endpoint {
                ip: e.ip,
                port: e.port,
                weight: Some(e.weight),
                address: String::new(), // Will be pre-computed in validate()
            });
        }

        upstreams.push(c::Upstream {
            name: u.name,
            load_balancer: lb,
            http_version: c::HttpVersion::H1, // Defaulting as binary doesn't store this yet
            connection_pool: c::ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints,
        });
    }

    let mut routes = Vec::new();
    for v in binary.routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let match_type = match p.match_type {
                MatchType::Exact => c::MatchType::Exact,
                MatchType::Regex => c::MatchType::Regex,
                MatchType::Prefix => c::MatchType::Prefix,
            };

            let request_headers = p.request_headers.map(|h| c::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let response_headers = p.response_headers.map(|h| c::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| c::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            paths.push(c::Route {
                match_type,
                path: p.path,
                timeout: None,
                retry: None,
                request_headers,
                response_headers,
                destinations,
                compiled_regex: None,
            });
        }

        routes.push(c::VirtualHost {
            host: v.host,
            paths,
        });
    }

    Config {
        server: c::ServerConfig {
            listen_addr: binary.listen_addr,
            worker_threads: None,
            tls: None,
        },
        telemetry: c::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: c::AccessLogConfig::False,
            tracing: None,
        },
        upstreams,
        routes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::config as c;
    use pavis_core::{Endpoint, LoadBalancer, PavisHeader, ProxyConfig, Upstream};

    #[test]
    fn test_convert_binary_to_config() {
        let binary = ProxyConfig {
            header: PavisHeader::default(),
            listen_addr: "0.0.0.0:8080".to_string(),
            upstreams: vec![Upstream {
                name: "test".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                endpoints: vec![Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 80,
                    weight: 1,
                }],
            }],
            routes: vec![],
        };

        let config = convert_binary_to_config(binary);

        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].name, "test");
        assert_eq!(config.upstreams[0].endpoints[0].ip, "127.0.0.1");
        // Check enum conversion
        assert_eq!(
            config.upstreams[0].load_balancer,
            c::LoadBalancer::RoundRobin
        );
    }
}
