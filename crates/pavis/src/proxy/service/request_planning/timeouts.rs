use pavis_core::{Duration as CoreDuration, RetryPolicy, Timeout, TryTimeout};

pub fn core_duration_to_std(duration: &CoreDuration) -> std::time::Duration {
    std::time::Duration::from_millis(duration.0.get() as u64)
}

pub fn resolve_route_timeout(timeout: Timeout) -> Option<std::time::Duration> {
    match timeout {
        Timeout::Enabled(duration) => Some(core_duration_to_std(&duration)),
        Timeout::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn resolve_per_try_timeout(
    timeout: Timeout,
    retry: &RetryPolicy,
) -> Option<std::time::Duration> {
    match retry {
        RetryPolicy::Enabled { per_try, .. } => match per_try {
            TryTimeout::Enabled(duration) => Some(core_duration_to_std(duration)),
            TryTimeout::Inherit => resolve_route_timeout(timeout),
            TryTimeout::Disabled => None,
            _ => None,
        },
        RetryPolicy::Disabled => resolve_route_timeout(timeout),
        _ => resolve_route_timeout(timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{BackoffStrategy, Duration};
    use std::num::NonZeroU32;

    #[test]
    fn test_resolve_route_timeout() {
        assert_eq!(resolve_route_timeout(Timeout::Disabled), None);
        assert_eq!(
            resolve_route_timeout(Timeout::Enabled(Duration(NonZeroU32::new(100).unwrap()))),
            Some(std::time::Duration::from_millis(100))
        );
    }

    #[test]
    fn test_resolve_per_try_timeout() {
        let route_timeout = Timeout::Enabled(Duration(NonZeroU32::new(500).unwrap()));
        let retry = RetryPolicy::Enabled {
            max_attempts: std::num::NonZeroU16::new(3).unwrap(),
            per_try: TryTimeout::Enabled(CoreDuration(NonZeroU32::new(100).unwrap())),
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategy::Fixed { base_ms: 10 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1024,
        };
        assert_eq!(
            resolve_per_try_timeout(route_timeout, &retry),
            Some(std::time::Duration::from_millis(100))
        );

        let retry_inherit = RetryPolicy::Enabled {
            max_attempts: std::num::NonZeroU16::new(3).unwrap(),
            per_try: TryTimeout::Inherit,
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategy::Fixed { base_ms: 10 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1024,
        };
        assert_eq!(
            resolve_per_try_timeout(route_timeout, &retry_inherit),
            Some(std::time::Duration::from_millis(500))
        );

        let retry_disabled = RetryPolicy::Disabled;
        assert_eq!(
            resolve_per_try_timeout(route_timeout, &retry_disabled),
            Some(std::time::Duration::from_millis(500))
        );
    }
}
