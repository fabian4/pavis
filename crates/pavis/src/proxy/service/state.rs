use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use arc_swap::ArcSwap;
use rustls::RootCertStore;
use std::sync::Arc;

pub struct Proxy {
    pub state: Arc<RuntimeStateHandle>,
    pub telemetry: Arc<Telemetry>,
    pub ca_store: Arc<ArcSwap<RootCertStore>>,
}

impl Proxy {}
