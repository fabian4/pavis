use crate::proxy::context::RequestId;
use http::header::{HeaderName, HeaderValue};
use opentelemetry::propagation::Injector;
use pingora::http::RequestHeader;
use rand::Rng;
use std::sync::atomic::{AtomicBool, Ordering};

static CLOCK_UNDERFLOW_WARNED: AtomicBool = AtomicBool::new(false);

pub struct HeaderInjector<'a>(pub &'a mut RequestHeader);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(key), value.parse::<HeaderValue>()) {
            let _ = self.0.insert_header(name, value);
        }
    }
}

pub fn request_id_timestamp(now: std::time::SystemTime) -> u128 {
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(err) => {
            if !CLOCK_UNDERFLOW_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    error = %err,
                    "System clock is before UNIX_EPOCH; using 0 for request id timestamp"
                );
            }
            0
        }
    }
}

pub fn generate_request_id() -> RequestId {
    let now = request_id_timestamp(std::time::SystemTime::now());
    let random_val: u32 = rand::rng().random();
    RequestId::from_parts(now, random_val)
}

pub fn clock_underflow_warned() -> &'static AtomicBool {
    &CLOCK_UNDERFLOW_WARNED
}

pub fn reset_clock_underflow_warned() {
    CLOCK_UNDERFLOW_WARNED.store(false, Ordering::Relaxed);
}
