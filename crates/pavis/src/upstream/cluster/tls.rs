use pingora::protocols::tls::CaType;
use pingora::utils::tls::CertKey;
use reqwest::{Certificate as ReqwestCertificate, Identity as ReqwestIdentity};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(crate) struct TlsMaterials {
    client_cert_key: Option<Arc<CertKey>>,
    ca_bundle: Option<Arc<CaType>>,
    health_identity: Option<Arc<ReqwestIdentity>>,
    health_root_certificates: Arc<Vec<ReqwestCertificate>>,
}

impl TlsMaterials {
    pub(crate) fn new(
        client_cert_key: Option<Arc<CertKey>>,
        ca_bundle: Option<Arc<CaType>>,
        health_identity: Option<Arc<ReqwestIdentity>>,
        health_root_certificates: Arc<Vec<ReqwestCertificate>>,
    ) -> Self {
        Self {
            client_cert_key,
            ca_bundle,
            health_identity,
            health_root_certificates,
        }
    }

    pub(crate) fn client_cert_key(&self) -> Option<Arc<CertKey>> {
        self.client_cert_key.clone()
    }

    pub(crate) fn ca_bundle(&self) -> Option<Arc<CaType>> {
        self.ca_bundle.clone()
    }

    pub(crate) fn health_identity(&self) -> Option<Arc<ReqwestIdentity>> {
        self.health_identity.clone()
    }

    pub(crate) fn health_root_certificates(&self) -> Arc<Vec<ReqwestCertificate>> {
        Arc::clone(&self.health_root_certificates)
    }
}
