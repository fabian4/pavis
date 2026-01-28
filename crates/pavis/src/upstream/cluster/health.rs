use pavis_core::{Endpoint, EndpointAddr};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct EndpointKey(String);

impl EndpointKey {
    pub(crate) fn from_endpoint(endpoint: &Endpoint) -> Self {
        Self::from_addr(&endpoint.address)
    }

    pub(crate) fn from_addr(addr: &EndpointAddr) -> Self {
        match addr {
            EndpointAddr::Ip { address, port } => Self(format!("{}:{}", address, port.0.get())),
            EndpointAddr::Dns { host, port } => Self(format!("{}:{}", host.0, port.0.get())),
            #[allow(unreachable_patterns)]
            _ => Self("unknown".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveHealth {
    Healthy,
    Unhealthy,
}

#[derive(Debug, Clone)]
pub(crate) struct EndpointStatus {
    consecutive_errors: u32,
    ejected_until: Option<Instant>,
    active_health: ActiveHealth,
}

impl Default for EndpointStatus {
    fn default() -> Self {
        Self {
            consecutive_errors: 0,
            ejected_until: None,
            active_health: ActiveHealth::Healthy,
        }
    }
}

#[derive(Debug)]
pub(crate) struct HealthState {
    pub(crate) endpoints: Vec<Endpoint>,
    statuses: HashMap<EndpointKey, EndpointStatus>,
}

impl HealthState {
    pub(crate) fn new(endpoints: Vec<Endpoint>) -> Self {
        let statuses = endpoints
            .iter()
            .map(|ep| (EndpointKey::from_endpoint(ep), EndpointStatus::default()))
            .collect();
        Self {
            endpoints,
            statuses,
        }
    }

    pub(crate) fn update_endpoints(&mut self, endpoints: Vec<Endpoint>) {
        let mut statuses = HashMap::new();
        for endpoint in &endpoints {
            let key = EndpointKey::from_endpoint(endpoint);
            let status = self.statuses.remove(&key).unwrap_or_default();
            statuses.insert(key, status);
        }
        self.endpoints = endpoints;
        self.statuses = statuses;
    }

    pub(crate) fn clone_endpoints(&self) -> Vec<Endpoint> {
        self.endpoints.clone()
    }

    pub(crate) fn mark_active_health(&mut self, endpoint: &EndpointAddr, healthy: bool) -> bool {
        let key = EndpointKey::from_addr(endpoint);
        if let Some(status) = self.statuses.get_mut(&key) {
            let next = if healthy {
                ActiveHealth::Healthy
            } else {
                ActiveHealth::Unhealthy
            };
            if status.active_health != next {
                status.active_health = next;
                return true;
            }
        }
        false
    }

    pub(crate) fn record_success(&mut self, endpoint: &EndpointAddr) -> bool {
        let key = EndpointKey::from_addr(endpoint);
        if let Some(status) = self.statuses.get_mut(&key)
            && status.consecutive_errors != 0
        {
            status.consecutive_errors = 0;
            return true;
        }
        false
    }

    pub(crate) fn record_failure(
        &mut self,
        endpoint: &EndpointAddr,
        threshold: u32,
        eject_for: std::time::Duration,
    ) -> bool {
        let key = EndpointKey::from_addr(endpoint);
        if let Some(status) = self.statuses.get_mut(&key) {
            status.consecutive_errors = status.consecutive_errors.saturating_add(1);
            if status.consecutive_errors >= threshold && status.ejected_until.is_none() {
                status.consecutive_errors = 0;
                status.ejected_until = Some(Instant::now() + eject_for);
                return true;
            }
        }
        false
    }

    pub(crate) fn clear_expired_ejections(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = false;
        for status in self.statuses.values_mut() {
            if let Some(until) = status.ejected_until
                && now >= until
            {
                status.ejected_until = None;
                status.consecutive_errors = 0;
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn eligible_endpoints(&self) -> Vec<Endpoint> {
        let now = Instant::now();
        self.endpoints
            .iter()
            .filter(|ep| {
                let key = EndpointKey::from_endpoint(ep);
                self.statuses
                    .get(&key)
                    .map(|status| {
                        status.active_health == ActiveHealth::Healthy
                            && status
                                .ejected_until
                                .map(|until| now >= until)
                                .unwrap_or(true)
                    })
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }
}
