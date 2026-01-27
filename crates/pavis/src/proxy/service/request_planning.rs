use crate::proxy::context::{RequestId, RouterContext};
use http::Uri;
use pavis_core::{EndpointAddr, Hostname, PathMatch, Principal, RetryPolicy, Timeout, TryTimeout};
use pingora::http::RequestHeader;
use pingora::prelude::*;
use rand::Rng;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};

static CLOCK_UNDERFLOW_WARNED: AtomicBool = AtomicBool::new(false);

pub struct HeaderInjector<'a>(pub &'a mut RequestHeader);

impl opentelemetry::propagation::Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::try_from(key),
            value.parse::<http::header::HeaderValue>(),
        ) {
            let _ = self.0.insert_header(name, value);
        }
    }
}

pub fn request_id_timestamp(now: std::time::SystemTime) -> u128 {
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(err) => {
            if !CLOCK_UNDERFLOW_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    error = %err,
                    "System clock is before UNIX_EPOCH; using 0 for request id timestamp"
                );
            }
            0
        }
    }
}

pub fn generate_request_id() -> RequestId {
    let now = request_id_timestamp(std::time::SystemTime::now());
    let random_val: u32 = rand::rng().random();
    RequestId::from_parts(now, random_val)
}

pub fn apply_route_headers(ctx: &mut RouterContext, route: &pavis_core::Route) {
    ctx.request_headers = route.request_headers.clone();
    ctx.response_headers = route.response_headers.clone();
    ctx.route_timeout = route.timeout;
    ctx.retry_policy = route.retry.clone();
    ctx.retry_attempts = 0;
}

pub fn core_duration_to_std(duration: &pavis_core::Duration) -> std::time::Duration {
    std::time::Duration::from_millis(duration.0.get() as u64)
}

pub fn reuse_key_hash(
    addr: &SocketAddr,
    sni: &str,
    verify_mode: Option<pavis_core::TlsVerify>,
    cert: Option<&pavis_core::ClientCert>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    addr.to_string().hash(&mut hasher);
    sni.hash(&mut hasher);
    let verify_tag = match verify_mode {
        Some(pavis_core::TlsVerify::Disabled) => 0u8,
        Some(pavis_core::TlsVerify::CaOnly) => 1u8,
        Some(pavis_core::TlsVerify::Full) => 2u8,
        _ => 3u8,
    };
    verify_tag.hash(&mut hasher);
    match cert {
        Some(pavis_core::ClientCert::Enabled {
            cert_path,
            key_path,
            chain,
        }) => {
            1u8.hash(&mut hasher);
            cert_path.0.hash(&mut hasher);
            key_path.0.hash(&mut hasher);
            match chain {
                pavis_core::ClientCertChain::None => 0u8.hash(&mut hasher),
                pavis_core::ClientCertChain::Embedded => 1u8.hash(&mut hasher),
                pavis_core::ClientCertChain::File { path } => {
                    2u8.hash(&mut hasher);
                    path.0.hash(&mut hasher);
                }
                #[allow(unreachable_patterns)]
                _ => 3u8.hash(&mut hasher),
            };
        }
        Some(pavis_core::ClientCert::Disabled) | None => {
            0u8.hash(&mut hasher);
        }
        #[allow(unreachable_patterns)]
        _ => {
            4u8.hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn resolve_route_timeout(timeout: Timeout) -> Option<std::time::Duration> {
    match timeout {
        Timeout::Enabled(duration) => Some(core_duration_to_std(&duration)),
        Timeout::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn resolve_per_try_timeout(
    timeout: Timeout,
    retry: &RetryPolicy,
) -> Option<std::time::Duration> {
    match retry {
        RetryPolicy::Enabled { per_try, .. } => match per_try {
            TryTimeout::Enabled(duration) => Some(core_duration_to_std(duration)),
            TryTimeout::Inherit => resolve_route_timeout(timeout),
            TryTimeout::Disabled => None,
            _ => None,
        },
        RetryPolicy::Disabled => resolve_route_timeout(timeout),
        _ => resolve_route_timeout(timeout),
    }
}

pub fn calculate_path_rewrite(
    route: &pavis_core::Route,
    uri_path: &str,
    uri_query: Option<&str>,
) -> Option<Uri> {
    match &route.rewrite.path {
        pavis_core::RewritePath::Disabled => None,
        pavis_core::RewritePath::Prefix { from: _, to } => {
            let new_path = match &route.matcher.path {
                PathMatch::Prefix { path } => {
                    uri_path.strip_prefix(path.0.as_str()).map(|suffix| {
                        let mut path = String::with_capacity(to.0.len() + suffix.len());
                        path.push_str(&to.0);
                        path.push_str(suffix);
                        Cow::Owned(path)
                    })
                }
                PathMatch::Exact { path } => {
                    (uri_path == path.0.as_str()).then_some(Cow::Borrowed(to.0.as_str()))
                }
                PathMatch::Regex { .. } => None,
                #[allow(unreachable_patterns)]
                &_ => None,
            };

            match new_path {
                Some(mut path) => {
                    if let Some(query) = uri_query {
                        let mut owned = match path {
                            Cow::Borrowed(path) => {
                                let mut owned = String::with_capacity(path.len() + 1 + query.len());
                                owned.push_str(path);
                                owned
                            }
                            Cow::Owned(mut owned) => {
                                owned.reserve(1 + query.len());
                                owned
                            }
                        };
                        owned.push('?');
                        owned.push_str(query);
                        path = Cow::Owned(owned);
                    }

                    match Uri::builder().path_and_query(path.as_ref()).build() {
                        Ok(uri) => Some(uri),
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                rewrite = %to.0,
                                "Failed to apply path rewrite"
                            );
                            None
                        }
                    }
                }
                None => {
                    if matches!(route.matcher.path, PathMatch::Regex { .. }) {
                        tracing::warn!(
                            route = %route_path(route),
                            "Skipping path rewrite for regex match"
                        );
                    } else {
                        tracing::warn!(
                            route = %route_path(route),
                            path = %uri_path,
                            "Skipping path rewrite due to unmatched prefix"
                        );
                    }
                    None
                }
            }
        }
        #[allow(unreachable_patterns)]
        &_ => None,
    }
}

pub fn route_path(route: &pavis_core::Route) -> &str {
    match &route.matcher.path {
        PathMatch::Prefix { path } => path.0.as_str(),
        PathMatch::Exact { path } => path.0.as_str(),
        PathMatch::Regex { path } => path.0.as_str(),
        #[allow(unreachable_patterns)]
        &_ => "",
    }
}

pub fn extract_client_identity(_session: &pingora::proxy::Session) -> Option<String> {
    // TODO: Implement peer certificate extraction for Rustls mode in Pingora 0.6.0.
    // The current Stream trait does not expose peer_certificate in a backend-agnostic way.
    None
}

pub fn resolve_sni(
    sni: &pavis_core::SniName,
    authority_override: Option<&Hostname>,
    endpoint_host: Option<&Hostname>,
) -> Option<Hostname> {
    match sni {
        pavis_core::SniName::Name(name) => Some(name.clone()),
        pavis_core::SniName::Auto => authority_override
            .cloned()
            .or_else(|| endpoint_host.cloned()),
        pavis_core::SniName::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn endpoint_host_for_sni(
    upstream: &pavis_core::Upstream,
    endpoint: &pavis_core::Endpoint,
) -> Option<Hostname> {
    match &endpoint.address {
        EndpointAddr::Dns { host, .. } => Some(host.clone()),
        EndpointAddr::Ip { .. } => {
            if matches!(
                upstream.discovery,
                pavis_core::Discovery::Logical | pavis_core::Discovery::Strict { .. }
            ) {
                let mut selected: Option<&Hostname> = None;
                for endpoint in &upstream.endpoints {
                    if let EndpointAddr::Dns { host, .. } = &endpoint.address {
                        match selected {
                            None => selected = Some(host),
                            Some(existing) => {
                                if existing.0 != host.0 {
                                    return None;
                                }
                            }
                        }
                    }
                }
                selected.cloned()
            } else {
                None
            }
        }
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn resolve_endpoint_addr(endpoint: &pavis_core::Endpoint) -> Result<SocketAddr> {
    match &endpoint.address {
        EndpointAddr::Ip { address, port } => Ok(SocketAddr::new(*address, port.0.get())),
        EndpointAddr::Dns { host, port } => {
            let mut addrs = match (host.0.as_str(), port.0.get()).to_socket_addrs() {
                Ok(addrs) => addrs,
                Err(err) => {
                    return Error::e_explain(
                        InternalError,
                        format!("DNS resolution failed for {}:{} ({})", host.0, port.0, err),
                    );
                }
            };
            match addrs.next() {
                Some(addr) => Ok(addr),
                None => Error::e_explain(
                    InternalError,
                    format!(
                        "DNS resolution returned no addresses for {}:{}",
                        host.0, port.0
                    ),
                ),
            }
        }
        #[allow(unreachable_patterns)]
        _ => Error::e_explain(InternalError, "Unknown endpoint address type"),
    }
}

pub fn is_authorized(principal: &Principal, client_identity: Option<&str>) -> bool {
    match principal {
        Principal::Any => true,
        Principal::Authenticated { spiffe } => {
            client_identity.is_some_and(|identity| identity == spiffe.as_str())
        }
        Principal::Prefix { prefix } => {
            client_identity.is_some_and(|identity| identity.starts_with(prefix.as_str()))
        }
        #[allow(unreachable_patterns)]
        _ => false,
    }
}

pub fn clock_underflow_warned() -> &'static AtomicBool {
    &CLOCK_UNDERFLOW_WARNED
}

pub fn reset_clock_underflow_warned() {
    CLOCK_UNDERFLOW_WARNED.store(false, Ordering::Relaxed);
}
