use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use std::sync::Arc;

pub struct Proxy {
    pub state: Arc<RuntimeStateHandle>,
    pub telemetry: Arc<Telemetry>,
}

impl Proxy {}
