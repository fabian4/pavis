use pavis::proxy::service::test_exports::resolve_per_try_timeout;
use pavis_core::{Duration, RetryPolicy, Timeout};
use std::num::{NonZeroU16, NonZeroU32};

#[test]
fn resolve_per_try_timeout_inherits_route_timeout() {
    let timeout = Timeout::Enabled(Duration(NonZeroU32::new(500).unwrap()));
    let retry = RetryPolicy::Enabled {
        max_attempts: NonZeroU16::new(2).unwrap(),
        per_try: pavis_core::TryTimeout::Inherit,
        retryable_reasons: vec![pavis_core::RetryReason::StatusCode],
        retryable_status_codes: Some(pavis_core::RetryableStatusCodes {
            codes: vec![502, 503, 504],
        }),
        backoff: pavis_core::BackoffStrategy::Exponential {
            base_ms: 100,
            max_ms: 5000,
        },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1_048_576,
    };
    assert_eq!(
        resolve_per_try_timeout(timeout, &retry),
        Some(std::time::Duration::from_millis(500))
    );
}
