use crate::runtime::{RuntimeConfig, Upstream, VirtualHost};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreValidationError {
    #[error("duplicate upstream name: {0}")]
    DuplicateUpstream(String),
    #[error("empty upstream name")]
    EmptyUpstreamName,
    #[error("upstream {0} has endpoint with weight 0")]
    EndpointWeightZero(String),
    #[error("route '{0}' (host '{1}') references unknown upstream '{2}'")]
    UnknownDestination(String, String, String),
    #[error("route '{0}' (host '{1}') destination '{2}' has weight 0")]
    DestinationWeightZero(String, String, String),
}

pub type CoreValidationResult<T> = Result<T, CoreValidationError>;

/// Validate canonical invariants on a fully constructed `RuntimeConfig`.
/// This is intended to be called after parsing/adaptation and before runtime use.
pub fn validate_runtime(config: &RuntimeConfig) -> CoreValidationResult<()> {
    validate_upstreams(&config.upstreams)?;
    validate_routes(&config.routes, &config.upstreams)?;
    Ok(())
}

fn validate_upstreams(upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let mut names = HashSet::new();
    for u in upstreams {
        if u.name.is_empty() {
            return Err(CoreValidationError::EmptyUpstreamName);
        }
        if !names.insert(&u.name) {
            return Err(CoreValidationError::DuplicateUpstream(u.name.clone()));
        }
        for ep in &u.endpoints {
            if ep.weight == 0 {
                return Err(CoreValidationError::EndpointWeightZero(u.name.clone()));
            }
        }
    }
    Ok(())
}

fn validate_routes(routes: &[VirtualHost], upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.as_str()).collect();

    for vhost in routes {
        for route in &vhost.paths {
            validate_destinations(route, vhost, &upstream_names)?;
        }
    }
    Ok(())
}

fn validate_destinations(
    route: &crate::runtime::Route,
    vhost: &VirtualHost,
    upstream_names: &HashSet<&str>,
) -> CoreValidationResult<()> {
    for dest in &route.destinations {
        if !upstream_names.contains(dest.upstream.as_str()) {
            return Err(CoreValidationError::UnknownDestination(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
        if dest.weight == 0 {
            return Err(CoreValidationError::DestinationWeightZero(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
    }
    Ok(())
}
