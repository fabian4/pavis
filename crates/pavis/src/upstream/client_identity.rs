use anyhow::{Context, Result, anyhow, bail};
use openssl::pkey::PKey;
use openssl::x509::X509;
#[cfg(not(target_os = "macos"))]
use openssl::{pkcs12::Pkcs12, stack::Stack};
use pavis_core::{ClientCert, ClientCertChain, TlsPolicy, Upstream, UpstreamCa};
use pingora::protocols::tls::CaType;
use pingora::utils::tls::CertKey;
use reqwest::{Certificate as ReqwestCertificate, Identity as ReqwestIdentity};
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct ClientIdentityMaterials {
    pub client_cert_key: Option<Arc<CertKey>>,
    pub ca_bundle: Option<Arc<CaType>>,
    pub health_identity: Option<Arc<ReqwestIdentity>>,
    pub health_root_certificates: Arc<Vec<ReqwestCertificate>>,
}

pub struct ClientIdentityMaterializer;

type ClientIdentityParts = (Option<Arc<CertKey>>, Option<Arc<ReqwestIdentity>>);

impl ClientIdentityMaterializer {
    pub fn materialize(upstream: &Upstream) -> Result<ClientIdentityMaterials> {
        let mut materials = ClientIdentityMaterials::default();
        if let TlsPolicy::Enabled { cert, ca, .. } = &upstream.tls {
            let (client_cert_key, health_identity) =
                Self::load_client_cert(cert, &upstream.name.0)?;
            let (ca_bundle, health_roots) = Self::load_ca_materials(ca, &upstream.name.0)?;
            materials.client_cert_key = client_cert_key;
            materials.health_identity = health_identity;
            materials.ca_bundle = ca_bundle;
            materials.health_root_certificates = Arc::new(health_roots);
        }
        Ok(materials)
    }

    fn load_client_cert(cert: &ClientCert, upstream_name: &str) -> Result<ClientIdentityParts> {
        match cert {
            ClientCert::Disabled => Ok((None, None)),
            ClientCert::Enabled {
                cert_path,
                key_path,
                chain,
            } => {
                let cert_pem = fs::read(Path::new(&cert_path.0))
                    .with_context(|| format!("failed to read client cert {}", cert_path.0))?;
                let parsed = X509::stack_from_pem(&cert_pem)
                    .context("failed to parse client cert PEM bundle")?;
                if parsed.is_empty() {
                    bail!("client cert bundle is empty for upstream {}", upstream_name);
                }

                let mut selected = match chain {
                    ClientCertChain::Embedded => parsed.clone(),
                    ClientCertChain::None | ClientCertChain::File { .. } => {
                        if parsed.len() != 1 {
                            bail!("client cert must contain exactly one certificate");
                        }
                        vec![parsed[0].clone()]
                    }
                    _ => parsed.clone(),
                };

                let mut identity_chain = Vec::new();
                if let ClientCertChain::File { path } = chain {
                    let chain_pem = fs::read(Path::new(&path.0))
                        .with_context(|| format!("failed to read client cert chain {}", path.0))?;
                    let chain_certs = X509::stack_from_pem(&chain_pem)
                        .context("failed to parse client cert chain")?;
                    if chain_certs.is_empty() {
                        bail!("client cert chain is empty for upstream {}", upstream_name);
                    }
                    identity_chain = chain_certs.clone();
                    selected.extend(chain_certs);
                } else if matches!(chain, ClientCertChain::Embedded) {
                    identity_chain = selected.iter().skip(1).cloned().collect();
                }

                let key_pem = fs::read(Path::new(&key_path.0))
                    .with_context(|| format!("failed to read client key {}", key_path.0))?;
                let key = PKey::private_key_from_pem(&key_pem)
                    .context("failed to parse client key PEM")?;

                let leaf = selected
                    .first()
                    .cloned()
                    .ok_or_else(|| anyhow!("missing client certificate"))?;

                let cert_key = Arc::new(CertKey::new(selected.clone(), key.clone()));
                let identity = Self::build_reqwest_identity(leaf, key, identity_chain)?;
                Ok((Some(cert_key), identity))
            }
            _ => Ok((None, None)),
        }
    }

    fn load_ca_materials(
        ca: &UpstreamCa,
        upstream_name: &str,
    ) -> Result<(Option<Arc<CaType>>, Vec<ReqwestCertificate>)> {
        match ca {
            UpstreamCa::System => Ok((None, Vec::new())),
            UpstreamCa::File { path } => {
                let pem = fs::read(Path::new(&path.0))
                    .with_context(|| format!("failed to read CA bundle {}", path.0))?;
                let certs = X509::stack_from_pem(&pem).context("failed to parse CA bundle")?;
                if certs.is_empty() {
                    bail!("CA bundle is empty at {}", path.0);
                }
                let ca_list: CaType = certs.into_boxed_slice();
                let reqwest_cert = ReqwestCertificate::from_pem(&pem)
                    .context("failed to parse CA bundle for reqwest")?;
                Ok((Some(Arc::new(ca_list)), vec![reqwest_cert]))
            }
            _ => {
                tracing::debug!(upstream = upstream_name, "using system CA bundle");
                Ok((None, Vec::new()))
            }
        }
    }

    fn build_reqwest_identity(
        leaf: X509,
        key: PKey<openssl::pkey::Private>,
        chain: Vec<X509>,
    ) -> Result<Option<Arc<ReqwestIdentity>>> {
        #[cfg(target_os = "macos")]
        {
            let _ = chain;
            let leaf_pem = leaf
                .to_pem()
                .context("failed to encode leaf cert for health checks")?;
            let key_pkcs8 = key
                .private_key_to_pem_pkcs8()
                .context("failed to convert client key to PKCS8")?;
            let identity = ReqwestIdentity::from_pkcs8_pem(&leaf_pem, &key_pkcs8)
                .context("failed to parse client identity for health checks")?;
            Ok(Some(Arc::new(identity)))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut chain = chain;
            let mut builder = Pkcs12::builder();
            builder.name("pavis-health").pkey(&key).cert(&leaf);
            if !chain.is_empty() {
                let mut ca_stack = Stack::new().context("failed to allocate CA stack")?;
                for cert in chain.drain(..) {
                    ca_stack.push(cert).context("failed to add chain cert")?;
                }
                builder.ca(ca_stack);
            }
            const PKCS12_PASSWORD: &str = "pavis";
            let pkcs12 = builder
                .build2(PKCS12_PASSWORD)
                .context("failed to build client PKCS12 identity")?;
            let identity = ReqwestIdentity::from_pkcs12_der(&pkcs12.to_der()?, PKCS12_PASSWORD)
                .context("failed to parse client identity for health checks")?;
            Ok(Some(Arc::new(identity)))
        }
    }
}
