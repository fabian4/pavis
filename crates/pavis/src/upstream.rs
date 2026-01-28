//! Upstream module: Backend cluster management and load balancing.
//!
//! # Architectural Invariants
//!
//! 1. **Thread-Safe Selection**: Endpoint selection must be thread-safe and highly concurrent.
//! 2. **Atomic Updates**: Dynamic updates to upstream state must be atomic or eventually consistent without blocking readers.
//! 3. **Distributed State**: Load balancing state (e.g., RR counters) should be distributed or aligned to prevent false sharing.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use pavis_core::Upstream;

mod client_identity;
pub mod cluster;
pub mod health;
pub mod load_balance;
pub mod resolver;

use client_identity::ClientIdentityMaterializer;
pub use cluster::Cluster;
pub use health::UpstreamHealthMonitor;
pub use resolver::UpstreamResolver;

pub struct Manager {
    clusters: HashMap<String, Arc<Cluster>>,
}

impl Manager {
    pub fn new(upstreams: &[Upstream]) -> Result<Self> {
        let mut clusters = HashMap::new();
        for upstream in upstreams {
            let materials = ClientIdentityMaterializer::materialize(upstream)?;
            let cluster = Cluster::new_with_tls_materials(upstream.clone(), materials);
            clusters.insert(upstream.name.0.clone(), Arc::new(cluster));
        }
        Ok(Self { clusters })
    }

    pub fn get(&self, name: &str) -> Option<Arc<Cluster>> {
        self.clusters.get(name).map(Arc::clone)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, Arc<Cluster>)> {
        self.clusters
            .iter()
            .map(|(name, cluster)| (name, Arc::clone(cluster)))
    }
}

#[cfg(test)]
mod tests {
    use super::Manager;
    use pavis_core::{
        ClientCertChain, ConnectTimeout, ConnectionLimit, Endpoint, EndpointAddr, HttpVersion,
        IdleTimeout, LoadBalancer, Pool, Port, TlsPolicy, TlsVerify, Upstream, UpstreamBuilder,
        UpstreamCa, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::{NonZeroU16, NonZeroU32};
    use std::path::PathBuf;

    #[test]
    fn manager_returns_configured_cluster() {
        let upstreams = vec![
            UpstreamBuilder::new()
                .id(UpstreamId(NonZeroU16::new(1).unwrap()))
                .name(UpstreamName("backend".to_string()))
                .discovery(pavis_core::Discovery::Static)
                .balancer(LoadBalancer::RoundRobin)
                .protocol(HttpVersion::H1)
                .pool(Pool {
                    idle: IdleTimeout::Disabled,
                    connect: ConnectTimeout::Disabled,
                    max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                    ..Pool::default()
                })
                .tls(TlsPolicy::Disabled)
                .add_endpoint(Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        port: Port(NonZeroU16::new(8080).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                })
                .build()
                .expect("upstream"),
        ];

        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("backend");
        assert!(cluster.is_some());
        assert_eq!(cluster.unwrap().config.name.0, "backend");
    }

    fn write_pem(path: &std::path::Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("write pem");
    }

    fn write_pem_bundle(path: &std::path::Path, certs: &[String]) {
        let mut bundle = String::new();
        for cert in certs {
            bundle.push_str(cert);
        }
        write_pem(path, bundle.as_bytes());
    }

    // Pure-Rust replacement for OpenSSL cert generation
    fn build_self_signed_cert() -> (String, String) {
        let mut params = rcgen::CertificateParams::new(vec!["client".to_string()]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "client");
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        (key_pair.serialize_pem(), cert.pem())
    }

    fn mtls_upstream(cert_path: PathBuf, key_path: PathBuf) -> Upstream {
        mtls_upstream_with_chain(cert_path, key_path, ClientCertChain::None)
    }

    fn mtls_upstream_with_chain(
        cert_path: PathBuf,
        key_path: PathBuf,
        chain: ClientCertChain,
    ) -> Upstream {
        UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("mtls".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: pavis_core::SniName::Auto,
                canonical_sni: pavis_core::CanonicalSni::Disabled,
                reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
                cert: pavis_core::ClientCert::Enabled {
                    cert_path: pavis_core::Path(cert_path.to_string_lossy().to_string()),
                    key_path: pavis_core::Path(key_path.to_string_lossy().to_string()),
                    chain,
                },
                ca: UpstreamCa::System,
            })
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream")
    }

    #[test]
    fn manager_loads_client_cert_key_for_upstream() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_upstream_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");

        let (client_key_pem, client_cert_pem) = build_self_signed_cert();
        write_pem(&cert_path, client_cert_pem.as_bytes());
        write_pem(&key_path, client_key_pem.as_bytes());

        let upstreams = vec![mtls_upstream(cert_path, key_path)];
        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("mtls").expect("cluster");
        assert!(cluster.client_cert_key().is_some());
    }

    #[test]
    fn manager_fails_when_client_cert_missing() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_missing_cert_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("missing.pem");
        let key_path = dir.join("client.key");

        let (client_key_pem, _client_cert_pem) = build_self_signed_cert();
        write_pem(&key_path, client_key_pem.as_bytes());

        let upstreams = vec![mtls_upstream(cert_path, key_path)];
        let err = Manager::new(&upstreams).err().expect("manager should fail");
        assert!(
            err.chain()
                .any(|cause| cause.to_string().contains("failed to read client cert"))
        );
    }

    #[test]
    fn manager_fails_when_client_key_missing() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_missing_key_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("missing.key");

        let (_client_key_pem, client_cert_pem) = build_self_signed_cert();
        write_pem(&cert_path, client_cert_pem.as_bytes());

        let upstreams = vec![mtls_upstream(cert_path, key_path)];
        let err = Manager::new(&upstreams).err().expect("manager should fail");
        assert!(
            err.chain()
                .any(|cause| cause.to_string().contains("failed to read client key"))
        );
    }

    #[test]
    fn manager_fails_when_client_cert_invalid_pem() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_invalid_cert_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");

        std::fs::write(&cert_path, b"not a cert").expect("write invalid cert");
        let (client_key_pem, _client_cert_pem) = build_self_signed_cert();
        write_pem(&key_path, client_key_pem.as_bytes());

        let upstreams = vec![mtls_upstream(cert_path, key_path)];
        let err = Manager::new(&upstreams).err().expect("manager should fail");
        assert!(
            err.to_string()
                .contains("failed to load client certificate for upstream")
        );
    }

    #[test]
    fn manager_loads_client_cert_chain_file() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_chain_file_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");
        let chain_path = dir.join("chain.pem");

        let (client_key_pem, client_cert_pem) = build_self_signed_cert();
        let (_extra_key_pem, extra_cert_pem) = build_self_signed_cert();
        write_pem(&cert_path, client_cert_pem.as_bytes());
        write_pem(&key_path, client_key_pem.as_bytes());
        write_pem(&chain_path, extra_cert_pem.as_bytes());

        let upstreams = vec![mtls_upstream_with_chain(
            cert_path,
            key_path,
            ClientCertChain::File {
                path: pavis_core::Path(chain_path.to_string_lossy().to_string()),
            },
        )];
        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("mtls").expect("cluster");
        assert!(cluster.client_cert_key().is_some());
    }

    #[test]
    fn manager_fails_when_client_cert_has_multiple_certs_without_chain_mode() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_multi_cert_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");

        let (client_key_pem, client_cert_pem) = build_self_signed_cert();
        let (_extra_key_pem, extra_cert_pem) = build_self_signed_cert();
        write_pem_bundle(&cert_path, &[client_cert_pem, extra_cert_pem]);
        write_pem(&key_path, client_key_pem.as_bytes());

        let upstreams = vec![mtls_upstream(cert_path, key_path)];
        let err = Manager::new(&upstreams).err().expect("manager should fail");
        assert!(err.chain().any(|cause| {
            cause
                .to_string()
                .contains("client cert must contain exactly one")
        }));
    }

    #[test]
    fn manager_allows_embedded_client_cert_chain() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "pavis_mtls_embedded_chain_{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");

        let (client_key_pem, client_cert_pem) = build_self_signed_cert();
        let (_extra_key_pem, extra_cert_pem) = build_self_signed_cert();
        write_pem_bundle(&cert_path, &[client_cert_pem, extra_cert_pem]);
        write_pem(&key_path, client_key_pem.as_bytes());

        let upstreams = vec![mtls_upstream_with_chain(
            cert_path,
            key_path,
            ClientCertChain::Embedded,
        )];
        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("mtls").expect("cluster");
        assert!(cluster.client_cert_key().is_some());
    }

    #[test]
    fn manager_loads_ca_bundle_for_upstream() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_ca_bundle_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let ca_path = dir.join("ca.pem");
        let (_ca_key_pem, ca_cert_pem) = build_self_signed_cert();
        write_pem(&ca_path, ca_cert_pem.as_bytes());

        let upstreams = vec![
            UpstreamBuilder::new()
                .id(UpstreamId(NonZeroU16::new(1).unwrap()))
                .name(UpstreamName("ca".to_string()))
                .discovery(pavis_core::Discovery::Static)
                .balancer(LoadBalancer::RoundRobin)
                .protocol(HttpVersion::H1)
                .pool(Pool {
                    idle: IdleTimeout::Disabled,
                    connect: ConnectTimeout::Disabled,
                    max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                    ..Pool::default()
                })
                .tls(TlsPolicy::Enabled {
                    verify: TlsVerify::Full,
                    sni: pavis_core::SniName::Name(pavis_core::Hostname("example.com".to_string())),
                    canonical_sni: pavis_core::CanonicalSni::Disabled,
                    reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
                    cert: pavis_core::ClientCert::Disabled,
                    ca: UpstreamCa::File {
                        path: pavis_core::Path(ca_path.to_string_lossy().to_string()),
                    },
                })
                .add_endpoint(Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        port: Port(NonZeroU16::new(8080).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                })
                .build()
                .expect("upstream"),
        ];

        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("ca").expect("cluster");
        assert!(cluster.ca_bundle().is_some());
    }
}
