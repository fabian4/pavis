mod io;
mod request_planning;
mod state;
mod telemetry;

pub use request_planning::HeaderInjector;
pub use state::Proxy;

#[doc(hidden)]
pub mod test_exports {
    pub use super::request_planning::HeaderInjector;
    pub use super::request_planning::{
        apply_route_headers, calculate_path_rewrite, clock_underflow_warned, endpoint_host_for_sni,
        generate_request_id, is_authorized, request_id_timestamp, reset_clock_underflow_warned,
        resolve_endpoint_addr, resolve_per_try_timeout, resolve_route_timeout, resolve_sni,
        reuse_key_hash, route_path,
    };
    pub use super::state::Proxy;
}
