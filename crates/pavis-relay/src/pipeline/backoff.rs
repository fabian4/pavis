use crate::config::BackoffConfig;
use std::time::Duration;

pub struct Backoff {
    config: BackoffConfig,
    next: Duration,
}

impl Backoff {
    pub fn new(config: BackoffConfig) -> Self {
        Self {
            config,
            next: config.base_delay,
        }
    }

    pub fn reset(&mut self) {
        self.next = self.config.base_delay;
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        let next = delay.saturating_mul(2);
        self.next = std::cmp::min(next, self.config.max_delay);
        delay
    }
}
