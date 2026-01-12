//! Upstream module: Backend cluster management and load balancing.
//!
//! # Architectural Invariants
//!
//! 1. **Thread-Safe Selection**: Endpoint selection must be thread-safe and highly concurrent.
//! 2. **Atomic Updates**: Dynamic updates to upstream state must be atomic or eventually consistent without blocking readers.
//! 3. **Distributed State**: Load balancing state (e.g., RR counters) should be distributed or aligned to prevent false sharing.

use anyhow::{Context, Result};
use pingora::protocols::tls::CaType;
use pingora::tls::{pkey::PKey, x509::X509};
use pingora::utils::tls::CertKey;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use pavis_core::Upstream;

pub mod cluster;
pub mod load_balance;
pub mod resolver;

pub use cluster::Cluster;
pub use resolver::UpstreamResolver;

pub struct Manager {
    clusters: HashMap<String, Cluster>,
}

impl Manager {
    pub fn new(upstreams: &[Upstream]) -> Result<Self> {
        let mut clusters = HashMap::new();
        for u in upstreams {
            let (client_cert_key, ca_bundle) = match &u.tls {
                pavis_core::TlsPolicy::Enabled { cert, ca, .. } => {
                    let cert_key = match cert {
                        pavis_core::ClientCert::Disabled => None,
                        pavis_core::ClientCert::Enabled {
                            cert_path,
                            key_path,
                            chain,
                        } => Some(
                            load_client_cert_key(
                                Path::new(&cert_path.0),
                                Path::new(&key_path.0),
                                chain,
                            )
                            .with_context(|| {
                                format!(
                                    "failed to load client certificate for upstream {}",
                                    u.name.0
                                )
                            })?,
                        ),
                        #[allow(unreachable_patterns)]
                        _ => None,
                    };
                    let ca_bundle = match ca {
                        pavis_core::UpstreamCa::System => None,
                        pavis_core::UpstreamCa::File { path } => {
                            Some(load_ca_bundle(Path::new(&path.0)).with_context(|| {
                                format!("failed to load upstream CA bundle for {}", u.name.0)
                            })?)
                        }
                        #[allow(unreachable_patterns)]
                        _ => None,
                    };
                    (cert_key, ca_bundle)
                }
                _ => (None, None),
            };
            clusters.insert(
                u.name.0.clone(),
                Cluster::new_with_client_cert(u.clone(), client_cert_key, ca_bundle),
            );
        }
        Ok(Self { clusters })
    }

    pub fn get(&self, name: &str) -> Option<&Cluster> {
        self.clusters.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Cluster)> {
        self.clusters.iter()
    }
}

fn load_client_cert_key(
    cert_path: &Path,
    key_path: &Path,
    chain: &pavis_core::ClientCertChain,
) -> Result<Arc<CertKey>> {
    let cert_bytes = fs::read(cert_path)
        .with_context(|| format!("failed to read client cert {}", cert_path.display()))?;
    let key_bytes = fs::read(key_path)
        .with_context(|| format!("failed to read client key {}", key_path.display()))?;
    let certs =
        X509::stack_from_pem(&cert_bytes).context("failed to parse client cert PEM bundle")?;
    if certs.is_empty() {
        anyhow::bail!("client cert bundle is empty");
    }
    let mut selected = match chain {
        pavis_core::ClientCertChain::Embedded => certs,
        pavis_core::ClientCertChain::None | pavis_core::ClientCertChain::File { .. } => {
            if certs.len() != 1 {
                anyhow::bail!("client cert must contain exactly one certificate");
            }
            vec![certs[0].clone()]
        }
        #[allow(unreachable_patterns)]
        _ => certs,
    };
    if let pavis_core::ClientCertChain::File { path } = chain {
        let chain_bytes = fs::read(Path::new(&path.0))
            .with_context(|| format!("failed to read client cert chain {}", path.0.as_str()))?;
        let chain_certs =
            X509::stack_from_pem(&chain_bytes).context("failed to parse client cert chain")?;
        if chain_certs.is_empty() {
            anyhow::bail!("client cert chain is empty");
        }
        selected.extend(chain_certs);
    }
    let key = PKey::private_key_from_pem(&key_bytes).context("failed to parse client key PEM")?;
    Ok(Arc::new(CertKey::new(selected, key)))
}

fn load_ca_bundle(path: &Path) -> Result<Arc<CaType>> {
    let ca_bytes =
        fs::read(path).with_context(|| format!("failed to read CA bundle {}", path.display()))?;
    let certs = X509::stack_from_pem(&ca_bytes).context("failed to parse CA bundle")?;
    if certs.is_empty() {
        anyhow::bail!("CA bundle is empty");
    }
    Ok(Arc::new(certs.into_boxed_slice()))
}

#[cfg(test)]
mod tests {
    use super::Manager;
    use pavis_core::{
        ClientCertChain, ConnectTimeout, ConnectionLimit, Endpoint, EndpointAddr, HttpVersion,
        IdleTimeout, LoadBalancer, Pool, Port, TlsPolicy, TlsVerify, Upstream, UpstreamCa,
        UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;
    use std::path::PathBuf;

    #[test]
    fn manager_returns_configured_cluster() {
        let upstreams = vec![Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("backend".to_string()),
            discovery: pavis_core::Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }];

        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("backend");
        assert!(cluster.is_some());
        assert_eq!(cluster.unwrap().config.name.0, "backend");
    }

    fn write_pem(path: &std::path::Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("write pem");
    }

    fn write_pem_bundle(path: &std::path::Path, certs: &[openssl::x509::X509]) {
        let mut bundle = Vec::new();
        for cert in certs {
            bundle.extend_from_slice(&cert.to_pem().expect("cert pem"));
        }
        write_pem(path, &bundle);
    }

    fn build_self_signed_cert() -> (
        openssl::pkey::PKey<openssl::pkey::Private>,
        openssl::x509::X509,
    ) {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::{X509Builder, X509NameBuilder};

        let rsa = Rsa::generate(2048).expect("client key");
        let pkey = PKey::from_rsa(rsa).expect("client pkey");

        let mut name = X509NameBuilder::new().expect("client name");
        name.append_entry_by_text("CN", "client")
            .expect("client name cn");
        let name = name.build();

        let mut builder = X509Builder::new().expect("client builder");
        builder.set_version(2).expect("client version");
        builder.set_subject_name(&name).expect("client subject");
        builder.set_issuer_name(&name).expect("client issuer");
        builder.set_pubkey(&pkey).expect("client pubkey");
        builder
            .set_not_before(&Asn1Time::days_from_now(0).expect("client not_before"))
            .expect("client not_before set");
        builder
            .set_not_after(&Asn1Time::days_from_now(365).expect("client not_after"))
            .expect("client not_after set");
        builder
            .sign(&pkey, MessageDigest::sha256())
            .expect("client sign");

        (pkey, builder.build())
    }

    fn mtls_upstream(cert_path: PathBuf, key_path: PathBuf) -> Upstream {
        mtls_upstream_with_chain(cert_path, key_path, ClientCertChain::None)
    }

    fn mtls_upstream_with_chain(
        cert_path: PathBuf,
        key_path: PathBuf,
        chain: ClientCertChain,
    ) -> Upstream {
        Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("mtls".to_string()),
            discovery: pavis_core::Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: pavis_core::SniName::Auto,
                cert: pavis_core::ClientCert::Enabled {
                    cert_path: pavis_core::Path(cert_path.to_string_lossy().to_string()),
                    key_path: pavis_core::Path(key_path.to_string_lossy().to_string()),
                    chain,
                },
                ca: UpstreamCa::System,
            },
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }
    }

    #[test]
    fn manager_loads_client_cert_key_for_upstream() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_mtls_upstream_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let cert_path = dir.join("client.pem");
        let key_path = dir.join("client.key");

        let (client_key, client_cert) = build_self_signed_cert();
        write_pem(&cert_path, &client_cert.to_pem().expect("client cert pem"));
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );

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

        let (client_key, _client_cert) = build_self_signed_cert();
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );

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

        let (_client_key, client_cert) = build_self_signed_cert();
        write_pem(&cert_path, &client_cert.to_pem().expect("client cert pem"));

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
        let (client_key, _client_cert) = build_self_signed_cert();
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );

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

        let (client_key, client_cert) = build_self_signed_cert();
        let (_extra_key, extra_cert) = build_self_signed_cert();
        write_pem(&cert_path, &client_cert.to_pem().expect("client cert pem"));
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );
        write_pem(&chain_path, &extra_cert.to_pem().expect("chain cert pem"));

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

        let (client_key, client_cert) = build_self_signed_cert();
        let (_extra_key, extra_cert) = build_self_signed_cert();
        write_pem_bundle(&cert_path, &[client_cert, extra_cert]);
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );

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

        let (client_key, client_cert) = build_self_signed_cert();
        let (_extra_key, extra_cert) = build_self_signed_cert();
        write_pem_bundle(&cert_path, &[client_cert, extra_cert]);
        write_pem(
            &key_path,
            &client_key
                .private_key_to_pem_pkcs8()
                .expect("client key pem"),
        );

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
        let (_ca_key, ca_cert) = build_self_signed_cert();
        write_pem(&ca_path, &ca_cert.to_pem().expect("ca cert pem"));

        let upstreams = vec![Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("ca".to_string()),
            discovery: pavis_core::Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Enabled {
                verify: TlsVerify::Full,
                sni: pavis_core::SniName::Name(pavis_core::Hostname("example.com".to_string())),
                cert: pavis_core::ClientCert::Disabled,
                ca: UpstreamCa::File {
                    path: pavis_core::Path(ca_path.to_string_lossy().to_string()),
                },
            },
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }];

        let manager = Manager::new(&upstreams).expect("manager");
        let cluster = manager.get("ca").expect("cluster");
        assert!(cluster.ca_bundle().is_some());
    }
}
