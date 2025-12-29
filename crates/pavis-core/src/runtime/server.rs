use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::net::SocketAddr;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    pub worker_threads: Option<u64>, // Store as u64 in the serialized model to avoid narrowing.
    pub tls: Option<TlsConfig>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}
