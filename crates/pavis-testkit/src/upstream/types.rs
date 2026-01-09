use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct HealthResponse {
    pub ok: bool,
}

#[derive(Serialize)]
pub struct EchoResponse {
    pub instance_id: String,
    pub method: String,
    pub path: String,
    pub query: String,
    pub protocol: Option<String>,
    pub tls: TlsDetails,
    pub headers: BTreeMap<String, Vec<String>>,
    pub body_len: usize,
    pub remote_addr: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct TlsDetails {
    pub enabled: bool,
    pub version: Option<String>,
    pub sni: Option<String>,
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: u16,
    pub ok: bool,
}

#[derive(Serialize)]
pub struct DelayResponse {
    pub delayed_ms: u64,
}

#[derive(Serialize)]
pub struct StubResponse {
    pub error: &'static str,
    pub endpoint: &'static str,
    pub note: &'static str,
}
