use pavis_core::{
    ClientCert, ClientCertChain, Endpoint, EndpointAddr, Hostname, SniName, TlsVerify, Upstream,
};
use pingora::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

pub fn reuse_key_hash(
    addr: &SocketAddr,
    sni: &str,
    verify_mode: Option<TlsVerify>,
    cert: Option<&ClientCert>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    addr.to_string().hash(&mut hasher);
    sni.hash(&mut hasher);
    let verify_tag = match verify_mode {
        Some(TlsVerify::Disabled) => 0u8,
        Some(TlsVerify::CaOnly) => 1u8,
        Some(TlsVerify::Full) => 2u8,
        _ => 3u8,
    };
    verify_tag.hash(&mut hasher);
    match cert {
        Some(ClientCert::Enabled {
            cert_path,
            key_path,
            chain,
        }) => {
            1u8.hash(&mut hasher);
            cert_path.0.hash(&mut hasher);
            key_path.0.hash(&mut hasher);
            match chain {
                ClientCertChain::None => 0u8.hash(&mut hasher),
                ClientCertChain::Embedded => 1u8.hash(&mut hasher),
                ClientCertChain::File { path } => {
                    2u8.hash(&mut hasher);
                    path.0.hash(&mut hasher);
                }
                #[allow(unreachable_patterns)]
                _ => 3u8.hash(&mut hasher),
            };
        }
        Some(ClientCert::Disabled) | None => {
            0u8.hash(&mut hasher);
        }
        #[allow(unreachable_patterns)]
        _ => {
            4u8.hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn resolve_sni(
    sni: &SniName,
    authority_override: Option<&Hostname>,
    endpoint_host: Option<&Hostname>,
) -> Option<Hostname> {
    match sni {
        SniName::Name(name) => Some(name.clone()),
        SniName::Auto => authority_override
            .cloned()
            .or_else(|| endpoint_host.cloned()),
        SniName::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn endpoint_host_for_sni(upstream: &Upstream, endpoint: &Endpoint) -> Option<Hostname> {
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

pub fn resolve_endpoint_addr(endpoint: &Endpoint) -> Result<SocketAddr> {
    match &endpoint.address {
        EndpointAddr::Ip { address, port } => Ok(SocketAddr::new(*address, port.0.get())),
        EndpointAddr::Dns { host, port } => {
            tracing::error!(
                host = %host.0,
                port = port.0.get(),
                "DNS endpoints must be materialized before routing"
            );
            Error::e_explain(
                InternalError,
                format!(
                    "DNS endpoint {}:{} was not materialized during config load",
                    host.0, port.0
                ),
            )
        }
        #[allow(unreachable_patterns)]
        _ => Error::e_explain(InternalError, "Unknown endpoint address type"),
    }
}
