use std::time::Duration;

pub struct Backoff {
    base: Duration,
    max: Duration,
    jitter_ms: u64,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration, jitter_ms: u64) -> Self {
        Self {
            base,
            max,
            jitter_ms,
        }
    }

    pub(crate) fn next_delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt.min(10)).unwrap_or(u32::MAX);
        let exp = self.base.saturating_mul(factor);
        let capped = if exp > self.max { self.max } else { exp };
        let jitter = rand::random::<u64>() % (self.jitter_ms + 1);
        capped + Duration::from_millis(jitter)
    }
}

#[cfg(test)]
mod tests {
    use super::Backoff;
    use std::time::Duration;

    #[test]
    fn backoff_caps_at_max() {
        let backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0);
        let delay = backoff.next_delay(20);
        assert_eq!(delay, Duration::from_secs(30));
    }

    #[test]
    fn backoff_scales_from_base() {
        let backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 0);
        let delay = backoff.next_delay(2);
        assert_eq!(delay, Duration::from_secs(4));
    }
}
