//! Retry execution logic for P2 retry/timeout implementation
//!
//! This module implements:
//! - Retry loop with deadline tracking
//! - Backoff strategy execution (fixed, linear, exponential)
//! - Request body buffering and replay
//! - Idempotency constraint enforcement
//! - Retry metrics and observability

use crate::telemetry::metrics::MetricsHandle;
use pavis_core::{BackoffStrategy, BodyReplayability, MethodIdempotency, RetryPolicy, RetryReason};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, warn};

/// Retry context tracking state across retry attempts
pub struct RetryContext {
    /// Global deadline for entire request (including all retries)
    pub global_deadline: Instant,
    /// Current attempt number (1-indexed, 1 = initial attempt)
    pub attempt: u16,
    /// Maximum attempts allowed
    pub max_attempts: u16,
    /// Retry policy
    pub policy: RetryPolicy,
    /// Metrics handle for recording retry stats
    pub metrics: Option<Arc<MetricsHandle>>,
    /// Upstream name for metrics
    pub upstream_name: String,
}

impl RetryContext {
    /// Create new retry context with global deadline
    pub fn new(
        policy: RetryPolicy,
        request_timeout_ms: u64,
        metrics: Option<Arc<MetricsHandle>>,
        upstream_name: String,
    ) -> Self {
        let global_deadline = Instant::now() + Duration::from_millis(request_timeout_ms);
        let max_attempts = match &policy {
            RetryPolicy::Enabled { max_attempts, .. } => max_attempts.get(),
            RetryPolicy::Disabled => 1,
            &_ => 1,
        };

        Self {
            global_deadline,
            attempt: 1,
            max_attempts,
            policy,
            metrics,
            upstream_name,
        }
    }

    /// Calculate remaining budget until global deadline
    pub fn remaining_budget(&self) -> Duration {
        self.global_deadline
            .saturating_duration_since(Instant::now())
    }

    /// Check if global deadline has been exceeded
    pub fn is_deadline_exceeded(&self) -> bool {
        Instant::now() >= self.global_deadline
    }

    /// Check if more retry attempts are available
    pub fn can_retry(&self) -> bool {
        self.attempt < self.max_attempts && !self.is_deadline_exceeded()
    }

    /// Increment attempt counter and record metric
    pub fn next_attempt(&mut self, reason: RetryReason) {
        self.attempt += 1;
        if let Some(metrics) = &self.metrics {
            let reason_str = match reason {
                RetryReason::StatusCode => "status_code",
                RetryReason::ConnectTimeout => "connect_timeout",
                RetryReason::ReadTimeout => "read_timeout",
                RetryReason::PerTryTimeout => "per_try_timeout",
                RetryReason::PoolFull => "pool_full",
                RetryReason::ConnectError => "connect_error",
            };
            metrics.record_retry(&self.upstream_name, reason_str, self.attempt);
        }
    }

    /// Record final outcome of retries
    pub fn record_outcome(&self, success: bool) {
        if let Some(metrics) = &self.metrics {
            let outcome = if success { "success" } else { "exhausted" };
            metrics.record_retry_outcome(&self.upstream_name, outcome);
        }
    }

    /// Check if a failure reason is retryable according to policy
    pub fn is_retryable(&self, reason: RetryReason) -> bool {
        match &self.policy {
            RetryPolicy::Disabled => false,
            RetryPolicy::Enabled {
                retryable_reasons, ..
            } => retryable_reasons.contains(&reason),
            &_ => false,
        }
    }

    /// Check if a status code is retryable according to policy
    pub fn is_status_code_retryable(&self, status: u16) -> bool {
        match &self.policy {
            RetryPolicy::Disabled => false,
            RetryPolicy::Enabled {
                retryable_reasons,
                retryable_status_codes,
                ..
            } => {
                if !retryable_reasons.contains(&RetryReason::StatusCode) {
                    return false;
                }

                retryable_status_codes
                    .as_ref()
                    .map(|codes| codes.codes.contains(&status))
                    .unwrap_or(false)
            }
            &_ => false,
        }
    }

    /// Check if method is allowed to retry based on idempotency rules
    pub fn is_method_allowed(&self, method: &pavis_core::HttpMethod) -> bool {
        match &self.policy {
            RetryPolicy::Disabled => false,
            RetryPolicy::Enabled {
                retry_non_idempotent,
                ..
            } => {
                let idempotency = MethodIdempotency::from_method(method);
                match idempotency {
                    MethodIdempotency::Idempotent => true,
                    MethodIdempotency::NonIdempotent => *retry_non_idempotent,
                }
            }
            &_ => false,
        }
    }

    /// Calculate backoff delay for current attempt
    pub fn calculate_backoff(&self) -> Duration {
        match &self.policy {
            RetryPolicy::Disabled => Duration::ZERO,
            RetryPolicy::Enabled { backoff, .. } => {
                let delay_ms = match backoff {
                    BackoffStrategy::Fixed { base_ms } => *base_ms,
                    BackoffStrategy::Linear { base_ms } => {
                        base_ms.saturating_mul((self.attempt - 1) as u64)
                    }
                    BackoffStrategy::Exponential { base_ms, max_ms } => {
                        if self.attempt < 2 {
                            0
                        } else {
                            let exponent = (self.attempt - 2) as u32;
                            let delay = base_ms.saturating_mul(2u64.saturating_pow(exponent));
                            delay.min(*max_ms)
                        }
                    }
                    &_ => 100,
                };

                Duration::from_millis(delay_ms)
            }
            &_ => Duration::ZERO,
        }
    }

    /// Apply backoff delay (sleep), respecting global deadline
    pub async fn apply_backoff(&self) {
        if self.attempt <= 1 {
            return; // No backoff before first retry
        }

        let backoff_delay = self.calculate_backoff();
        let remaining = self.remaining_budget();

        if remaining.is_zero() {
            debug!("No budget remaining for backoff, skipping");
            return;
        }

        let actual_delay = backoff_delay.min(remaining);

        debug!(
            attempt = self.attempt,
            backoff_ms = actual_delay.as_millis(),
            "Applying backoff delay"
        );

        sleep(actual_delay).await;
    }
}

/// Request body buffering for replay
pub struct BufferedBody {
    /// Buffered body bytes
    pub bytes: Vec<u8>,
    /// Replayability status
    pub replayability: BodyReplayability,
}

impl BufferedBody {
    /// Create new buffered body
    pub fn new(
        bytes: Vec<u8>,
        max_buffer_size: u64,
        metrics: Option<Arc<MetricsHandle>>,
        upstream_name: &str,
    ) -> Self {
        let replayability = if bytes.is_empty() {
            BodyReplayability::Empty
        } else if bytes.len() as u64 <= max_buffer_size {
            if let Some(metrics) = &metrics {
                metrics.record_retry_body_buffered(upstream_name, bytes.len() as u64);
            }
            BodyReplayability::Buffered
        } else {
            BodyReplayability::Streaming
        };

        Self {
            bytes,
            replayability,
        }
    }

    /// Check if body can be replayed
    pub fn can_replay(&self) -> bool {
        matches!(
            self.replayability,
            BodyReplayability::Empty | BodyReplayability::Buffered
        )
    }

    /// Handle non-replayable body according to policy
    pub fn handle_non_replayable(
        &self,
        fail_on_non_replayable: bool,
    ) -> Result<(), RetryBodyError> {
        if !self.can_replay() {
            if fail_on_non_replayable {
                return Err(RetryBodyError::NotReplayable);
            } else {
                warn!("Request body not replayable, retry aborted (returning last response)");
            }
        }
        Ok(())
    }
}

/// Retry body error
#[derive(Debug)]
pub enum RetryBodyError {
    /// Body is not replayable (streaming or exceeds buffer limit)
    NotReplayable,
}

impl std::fmt::Display for RetryBodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetryBodyError::NotReplayable => {
                write!(f, "request body is not replayable (streaming body)")
            }
        }
    }
}

impl std::error::Error for RetryBodyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::RetryableStatusCodes;
    use std::num::NonZeroU16;

    fn create_test_policy(max_attempts: u16) -> RetryPolicy {
        RetryPolicy::Enabled {
            max_attempts: NonZeroU16::new(max_attempts).unwrap(),
            per_try: pavis_core::TryTimeout::Inherit,
            retryable_reasons: vec![
                RetryReason::StatusCode,
                RetryReason::ConnectTimeout,
                RetryReason::ReadTimeout,
            ],
            retryable_status_codes: Some(RetryableStatusCodes {
                codes: vec![502, 503, 504],
            }),
            backoff: BackoffStrategy::Exponential {
                base_ms: 100,
                max_ms: 5000,
            },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        }
    }

    #[test]
    fn retry_context_tracks_attempts() {
        let policy = create_test_policy(3);
        let mut ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        assert_eq!(ctx.attempt, 1);
        assert_eq!(ctx.max_attempts, 3);
        assert!(ctx.can_retry());

        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.attempt, 2);
        assert!(ctx.can_retry());

        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.attempt, 3);
        assert!(!ctx.can_retry()); // No more attempts
    }

    #[test]
    fn retry_context_checks_retryable_reasons() {
        let policy = create_test_policy(3);
        let ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        assert!(ctx.is_retryable(RetryReason::StatusCode));
        assert!(ctx.is_retryable(RetryReason::ConnectTimeout));
        assert!(!ctx.is_retryable(RetryReason::PoolFull));
    }

    #[test]
    fn retry_context_checks_retryable_status_codes() {
        let policy = create_test_policy(3);
        let ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        assert!(ctx.is_status_code_retryable(502));
        assert!(ctx.is_status_code_retryable(503));
        assert!(ctx.is_status_code_retryable(504));
        assert!(!ctx.is_status_code_retryable(500));
        assert!(!ctx.is_status_code_retryable(404));
    }

    #[test]
    fn retry_context_checks_idempotency() {
        let policy = create_test_policy(3);
        let ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        assert!(ctx.is_method_allowed(&pavis_core::HttpMethod::GET));
        assert!(ctx.is_method_allowed(&pavis_core::HttpMethod::HEAD));
        assert!(!ctx.is_method_allowed(&pavis_core::HttpMethod::POST));
    }

    #[test]
    fn retry_context_calculates_exponential_backoff() {
        let policy = create_test_policy(5);
        let mut ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        // Attempt 1: no backoff
        assert_eq!(ctx.calculate_backoff(), Duration::ZERO);

        // Attempt 2: 100ms
        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.calculate_backoff(), Duration::from_millis(100));

        // Attempt 3: 200ms
        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.calculate_backoff(), Duration::from_millis(200));

        // Attempt 4: 400ms
        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.calculate_backoff(), Duration::from_millis(400));

        // Attempt 5: 800ms
        ctx.next_attempt(RetryReason::StatusCode);
        assert_eq!(ctx.calculate_backoff(), Duration::from_millis(800));
    }

    #[test]
    fn retry_context_caps_exponential_backoff() {
        let policy = RetryPolicy::Enabled {
            max_attempts: NonZeroU16::new(10).unwrap(),
            per_try: pavis_core::TryTimeout::Inherit,
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategy::Exponential {
                base_ms: 100,
                max_ms: 500,
            },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let mut ctx = RetryContext::new(policy, 10000, None, "test".to_string());

        // Advance to attempt 6 (would be 1600ms without cap)
        for _ in 0..5 {
            ctx.next_attempt(RetryReason::StatusCode);
        }

        let backoff = ctx.calculate_backoff();
        assert_eq!(backoff, Duration::from_millis(500)); // Capped at max_ms
    }

    #[test]
    fn buffered_body_empty() {
        let body = BufferedBody::new(vec![], 1024, None, "test");
        assert_eq!(body.replayability, BodyReplayability::Empty);
        assert!(body.can_replay());
    }

    #[test]
    fn buffered_body_buffered() {
        let body = BufferedBody::new(vec![1, 2, 3], 1024, None, "test");
        assert_eq!(body.replayability, BodyReplayability::Buffered);
        assert!(body.can_replay());
    }

    #[test]
    fn buffered_body_streaming() {
        let large_body = vec![0u8; 2048];
        let body = BufferedBody::new(large_body, 1024, None, "test");
        assert_eq!(body.replayability, BodyReplayability::Streaming);
        assert!(!body.can_replay());
    }

    #[test]
    fn buffered_body_handle_non_replayable_strict() {
        let body = BufferedBody::new(vec![0u8; 2048], 1024, None, "test");
        let result = body.handle_non_replayable(true);
        assert!(result.is_err());
    }

    #[test]
    fn buffered_body_handle_non_replayable_lenient() {
        let body = BufferedBody::new(vec![0u8; 2048], 1024, None, "test");
        let result = body.handle_non_replayable(false);
        assert!(result.is_ok());
    }

    #[test]
    fn retry_context_respects_global_deadline() {
        let policy = create_test_policy(3);
        // Short timeout: 50ms
        let ctx = RetryContext::new(policy, 50, None, "test".to_string());

        assert!(!ctx.is_deadline_exceeded());
        assert!(ctx.remaining_budget() > Duration::ZERO);

        // Wait for deadline
        std::thread::sleep(std::time::Duration::from_millis(60));

        assert!(ctx.is_deadline_exceeded());
        assert_eq!(ctx.remaining_budget(), Duration::ZERO);
        assert!(!ctx.can_retry());
    }

    #[tokio::test]
    async fn retry_context_skips_backoff_when_budget_exhausted() {
        let policy = create_test_policy(3);
        // 500ms budget
        let mut ctx = RetryContext::new(policy, 500, None, "test".to_string());

        // Attempt 2 would have 100ms backoff
        ctx.next_attempt(RetryReason::StatusCode);

        // Sleep to consume most of budget: 450ms
        std::thread::sleep(std::time::Duration::from_millis(450));

        let backoff = ctx.calculate_backoff();
        assert_eq!(backoff, Duration::from_millis(100));

        let remaining = ctx.remaining_budget();
        assert!(remaining < backoff);

        // apply_backoff should return quickly (clamped to remaining)
        let start = Instant::now();
        ctx.apply_backoff().await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
