use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Listener {
    pub name: String,
    pub address: String,
    pub workers: Option<u16>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TlsConfig {
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub client_auth: Option<ClientAuthConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ClientAuthConfig {
    Disabled,
    Optional { ca_path: String },
    Required { ca_path: String },
}
