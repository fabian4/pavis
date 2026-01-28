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
