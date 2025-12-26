use super::Config;
use anyhow::{Context, Result, anyhow};
use std::collections::HashSet;
use std::net::SocketAddr;

pub fn validate(config: &Config) -> Result<()> {
    validate_server(config)?;
    validate_upstreams(config)?;
    validate_routes(config)?;
    Ok(())
}

fn validate_server(config: &Config) -> Result<()> {
    // Validate listen_addr is a valid socket address
    // This catches invalid formats like "8080" (missing IP) or "localhost:8080" (if parse only accepts IP literals,
    // though std::net::ToSocketAddrs might be better if we want to support hostnames,
    // but SocketAddr::from_str is stricter and safer for a proxy listen address usually).
    // Config struct says listen_addr is String.
    config
        .server
        .listen_addr
        .parse::<SocketAddr>()
        .with_context(|| {
            format!(
                "Invalid listen_addr: '{}'. Must be IP:PORT (e.g., 0.0.0.0:8080)",
                config.server.listen_addr
            )
        })?;
    Ok(())
}

fn validate_upstreams(config: &Config) -> Result<()> {
    for upstream in &config.upstreams {
        for endpoint in &upstream.endpoints {
            if endpoint.weight == Some(0) {
                return Err(anyhow!(
                    "Upstream '{}' endpoint {}:{} has weight 0. Weight must be > 0.",
                    upstream.name,
                    endpoint.ip,
                    endpoint.port
                ));
            }
            if endpoint.ip.is_empty() {
                return Err(anyhow!(
                    "Upstream '{}' has endpoint with empty IP",
                    upstream.name
                ));
            }
            // Basic IP/Host check?
            // Since we construct address as format!("{}:{}", ip, port) later, we can check basic validity.
            if endpoint.ip.contains(':') && !endpoint.ip.starts_with('[') {
                // Might be IPv6 literal without brackets? Or just check if empty.
                // Let's stick to empty check for now to allow hostnames.
            }
        }
    }
    Ok(())
}

fn validate_routes(config: &Config) -> Result<()> {
    let upstream_names: HashSet<&String> = config.upstreams.iter().map(|u| &u.name).collect();

    for vhost in &config.routes {
        for route in &vhost.paths {
            for dest in &route.destinations {
                if !upstream_names.contains(&dest.upstream) {
                    return Err(anyhow!(
                        "Route '{}' (host: '{}') references unknown upstream: '{}'",
                        route.path,
                        vhost.host,
                        dest.upstream
                    ));
                }
                if dest.weight == 0 {
                    return Err(anyhow!(
                        "Route '{}' (host: '{}') destination '{}' has weight 0.",
                        route.path,
                        vhost.host,
                        dest.upstream
                    ));
                }
            }
        }
    }
    Ok(())
}
