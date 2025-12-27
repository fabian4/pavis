use super::{Config, HeaderOperations};
use anyhow::{Context, Result, anyhow};
use http::header::{HeaderName, HeaderValue};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;

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
    let mut names = HashSet::new();
    for upstream in &config.upstreams {
        if !names.insert(&upstream.name) {
            return Err(anyhow!(
                "Duplicate upstream name found: '{}'",
                upstream.name
            ));
        }
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
            if let Some(headers) = &route.request_headers {
                validate_headers(headers, &format!("Route '{}' request headers", route.path))?;
            }
            if let Some(headers) = &route.response_headers {
                validate_headers(headers, &format!("Route '{}' response headers", route.path))?;
            }

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

fn validate_headers(headers: &HeaderOperations, context: &str) -> Result<()> {
    if let Some(add) = &headers.add {
        for (k, v) in add {
            if k.is_empty() {
                return Err(anyhow!("{}: Header name cannot be empty", context));
            }

            HeaderName::from_str(k)
                .with_context(|| format!("{}: Invalid header name '{}'", context, k))?;

            // We allow spaces in values (RFC 7230), but we check for CRLF via from_str
            HeaderValue::from_str(v)
                .with_context(|| format!("{}: Invalid header value for '{}'", context, k))?;
        }
    }
    if let Some(remove) = &headers.remove {
        for k in remove {
            if k.is_empty() {
                return Err(anyhow!(
                    "{}: Header name to remove cannot be empty",
                    context
                ));
            }
            HeaderName::from_str(k)
                .with_context(|| format!("{}: Invalid header name to remove '{}'", context, k))?;
        }
    }
    Ok(())
}
