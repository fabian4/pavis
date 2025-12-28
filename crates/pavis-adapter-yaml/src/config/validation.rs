use super::{HeaderOperations, MatchType, YamlConfig};
use anyhow::{Context, Result, anyhow};
use http::header::{HeaderName, HeaderValue};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

pub fn validate(config: &mut YamlConfig) -> Result<()> {
    validate_server(config)?;
    validate_upstreams(config)?;
    validate_routes(config)?;
    Ok(())
}

fn validate_server(config: &YamlConfig) -> Result<()> {
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

fn validate_upstreams(config: &YamlConfig) -> Result<()> {
    let hostname_regex = regex::Regex::new(
        r"^([a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*)$",
    )
    .unwrap();

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

            // Validate IP or Hostname
            if IpAddr::from_str(&endpoint.ip).is_err() {
                // Not a valid IP, so it must be a valid hostname (RFC 1123).
                if !hostname_regex.is_match(&endpoint.ip) {
                    return Err(anyhow!(
                        "Upstream '{}' has invalid endpoint IP/hostname: '{}'. Must be a valid IP address or RFC 1123 hostname.",
                        upstream.name,
                        endpoint.ip
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_routes(config: &mut YamlConfig) -> Result<()> {
    let upstream_names: HashSet<&String> = config.upstreams.iter().map(|u| &u.name).collect();

    for vhost in &mut config.routes {
        for route in &mut vhost.paths {
            // Regex validation
            if route.match_type == MatchType::Regex {
                let compiled = regex::Regex::new(&route.path)
                    .with_context(|| format!("Invalid regex in route '{}'", route.path))?;
                route.compiled_regex = Some(compiled);
            }

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
