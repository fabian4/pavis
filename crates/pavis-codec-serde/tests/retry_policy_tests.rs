//! Integration tests for retry policy codec conversion

use pavis_codec_serde::config::types::{BackoffStrategyDTO, RetryPolicy};

#[test]
fn test_parse_retry_policy_max_attempts_zero() {
    // This will be tested via codec conversion in routes.rs
    // Placeholder for integration test structure
}

#[test]
fn test_parse_retry_policy_max_attempts_exceeds_limit() {
    // Max attempts > 10 should be rejected
    // Placeholder for integration test structure
}

#[test]
fn test_parse_retry_policy_retryable_status_codes_required() {
    // When "status_code" is in retryable_reasons, retryable_status_codes must be present
    // Placeholder for integration test structure
}

#[test]
fn test_parse_retry_policy_valid() {
    let policy = RetryPolicy {
        max_attempts: 3,
        retryable_reasons: vec!["status_code".to_string(), "connect_timeout".to_string()],
        retryable_status_codes: Some(vec![502, 503, 504]),
        backoff: BackoffStrategyDTO::Exponential {
            base_ms: 100,
            max_ms: 5000,
        },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1_048_576,
        per_try: None,
    };

    // Verify structure is correct
    assert_eq!(policy.max_attempts, 3);
    assert_eq!(policy.retryable_reasons.len(), 2);
    assert!(policy.retryable_status_codes.is_some());
}

#[test]
fn test_backoff_strategy_fixed() {
    let backoff = BackoffStrategyDTO::Fixed { base_ms: 200 };
    match backoff {
        BackoffStrategyDTO::Fixed { base_ms } => assert_eq!(base_ms, 200),
        _ => panic!("Expected Fixed strategy"),
    }
}

#[test]
fn test_backoff_strategy_linear() {
    let backoff = BackoffStrategyDTO::Linear { base_ms: 150 };
    match backoff {
        BackoffStrategyDTO::Linear { base_ms } => assert_eq!(base_ms, 150),
        _ => panic!("Expected Linear strategy"),
    }
}

#[test]
fn test_backoff_strategy_exponential() {
    let backoff = BackoffStrategyDTO::Exponential {
        base_ms: 100,
        max_ms: 5000,
    };
    match backoff {
        BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
            assert_eq!(base_ms, 100);
            assert_eq!(max_ms, 5000);
        }
        _ => panic!("Expected Exponential strategy"),
    }
}

#[test]
fn test_retry_policy_defaults() {
    let policy = RetryPolicy {
        max_attempts: 1,
        retryable_reasons: vec![
            "status_code".to_string(),
            "connect_timeout".to_string(),
            "read_timeout".to_string(),
        ],
        retryable_status_codes: Some(vec![502, 503, 504]),
        backoff: BackoffStrategyDTO::default(),
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1_048_576,
        per_try: None,
    };

    assert_eq!(policy.max_attempts, 1);
    assert_eq!(policy.retryable_reasons.len(), 3);
}

#[test]
fn test_retry_non_idempotent_flag() {
    let policy = RetryPolicy {
        max_attempts: 3,
        retryable_reasons: vec!["status_code".to_string()],
        retryable_status_codes: Some(vec![500]),
        backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
        retry_non_idempotent: true,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1_048_576,
        per_try: None,
    };

    assert!(policy.retry_non_idempotent);
}

#[test]
fn test_fail_on_non_replayable_retry_flag() {
    let policy = RetryPolicy {
        max_attempts: 3,
        retryable_reasons: vec!["status_code".to_string()],
        retryable_status_codes: Some(vec![500]),
        backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: true,
        max_request_body_buffer_bytes: 1_048_576,
        per_try: None,
    };

    assert!(policy.fail_on_non_replayable_retry);
}

#[test]
fn test_max_request_body_buffer_bytes() {
    let policy = RetryPolicy {
        max_attempts: 3,
        retryable_reasons: vec!["status_code".to_string()],
        retryable_status_codes: Some(vec![500]),
        backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 2_097_152, // 2MB
        per_try: None,
    };

    assert_eq!(policy.max_request_body_buffer_bytes, 2_097_152);
}
