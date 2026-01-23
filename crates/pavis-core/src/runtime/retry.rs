//! Retry policy types for P2 implementation
//!
//! This module defines the complete retry policy model including:
//! - BackoffStrategy (fixed, linear, exponential)
//! - RetryReason (status_code, timeouts, pool_full, etc.)
//! - Extended RetryPolicy with all P2 features
//! - Idempotency constraints
//! - Request body replayability types

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::num::NonZeroU16;

use super::TryTimeout;

/// Backoff strategy for retry delays
#[derive(
    Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone, PartialEq, Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    /// Delay = base_ms for all attempts
    Fixed {
        /// Base delay in milliseconds
        base_ms: u64,
    },

    /// Linear backoff
    /// Delay = base_ms * (attempt - 1)
    Linear {
        /// Base delay in milliseconds
        base_ms: u64,
    },

    /// Exponential backoff with cap
    /// Delay = min(base_ms * 2^(attempt - 2), max_ms)
    Exponential {
        /// Base delay in milliseconds
        base_ms: u64,
        /// Maximum delay cap in milliseconds
        max_ms: u64,
    },
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self::Exponential {
            base_ms: 100,
            max_ms: 5000,
        }
    }
}

/// Retry reason enum for observability and filtering
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
#[repr(u8)]
pub enum RetryReason {
    /// HTTP status code triggered retry (e.g., 502, 503, 504)
    StatusCode,
    /// Connect timeout exceeded
    ConnectTimeout,
    /// Read timeout exceeded
    ReadTimeout,
    /// Per-try timeout exceeded
    PerTryTimeout,
    /// Connection pool was full
    PoolFull,
    /// Connection error (TCP/TLS handshake failure)
    ConnectError,
}

/// Retryable status codes configuration
#[derive(
    Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone, PartialEq, Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RetryableStatusCodes {
    /// List of HTTP status codes that trigger retries
    /// Example: [502, 503, 504]
    pub codes: Vec<u16>,
}

impl Default for RetryableStatusCodes {
    fn default() -> Self {
        Self {
            codes: vec![502, 503, 504],
        }
    }
}

/// Retry policy with full P2 features
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum RetryPolicy {
    /// No retries
    #[default]
    Disabled,

    /// Full retry policy with all P2 features
    Enabled {
        /// Maximum attempts (including initial attempt)
        /// Valid range: 1..=10
        /// Default: 1 (no retries)
        max_attempts: NonZeroU16,

        /// Per-try timeout configuration
        /// Default: Inherit (use route timeout)
        per_try: TryTimeout,

        /// Retryable reasons (which failure types should trigger retries)
        /// Default: [StatusCode, ConnectTimeout, ReadTimeout]
        retryable_reasons: Vec<RetryReason>,

        /// Retryable status codes (only used if StatusCode is in retryable_reasons)
        /// Required when StatusCode is in retryable_reasons
        retryable_status_codes: Option<RetryableStatusCodes>,

        /// Backoff strategy
        /// Default: Exponential { base_ms: 100, max_ms: 5000 }
        backoff: BackoffStrategy,

        /// Enable retries for non-idempotent methods (POST, PUT, PATCH, DELETE)
        /// Default: false (only GET, HEAD, OPTIONS, TRACE are retried)
        /// WARNING: Enabling this requires request body buffering
        retry_non_idempotent: bool,

        /// Fail with 500 if retry is required but body is not replayable
        /// Default: false (return last response instead)
        fail_on_non_replayable_retry: bool,

        /// Maximum request body size to buffer in memory for replay
        /// Bodies larger than this are streaming and not replayable
        /// Default: 1MB
        /// 0 = disable buffering (no body replay)
        max_request_body_buffer_bytes: u64,
    },
}

/// HTTP method idempotency classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodIdempotency {
    /// Idempotent methods (safe to retry): GET, HEAD, OPTIONS, TRACE
    Idempotent,
    /// Non-idempotent methods (requires explicit flag): POST, PUT, PATCH, DELETE
    NonIdempotent,
}

impl MethodIdempotency {
    /// Classify HTTP method by idempotency
    pub fn from_method(method: &crate::runtime::HttpMethod) -> Self {
        use crate::runtime::HttpMethod;
        match method {
            HttpMethod::GET | HttpMethod::HEAD | HttpMethod::OPTIONS | HttpMethod::TRACE => {
                Self::Idempotent
            }
            HttpMethod::POST
            | HttpMethod::PUT
            | HttpMethod::PATCH
            | HttpMethod::DELETE
            | HttpMethod::CONNECT => Self::NonIdempotent,
        }
    }
}

/// Request body replayability status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyReplayability {
    /// Body is buffered in memory and can be replayed
    Buffered,
    /// Body is streaming (chunked or exceeds buffer limit)
    Streaming,
    /// No body present
    Empty,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_strategy_default() {
        let strategy = BackoffStrategy::default();
        match strategy {
            BackoffStrategy::Exponential { base_ms, max_ms } => {
                assert_eq!(base_ms, 100);
                assert_eq!(max_ms, 5000);
            }
            _ => panic!("Expected Exponential strategy"),
        }
    }

    #[test]
    fn retryable_status_codes_default() {
        let codes = RetryableStatusCodes::default();
        assert_eq!(codes.codes, vec![502, 503, 504]);
    }

    #[test]
    fn retry_policy_default_is_disabled() {
        let policy = RetryPolicy::default();
        assert!(matches!(policy, RetryPolicy::Disabled));
    }

    #[test]
    fn method_idempotency_get_is_idempotent() {
        use crate::runtime::HttpMethod;
        assert_eq!(
            MethodIdempotency::from_method(&HttpMethod::GET),
            MethodIdempotency::Idempotent
        );
    }

    #[test]
    fn method_idempotency_post_is_non_idempotent() {
        use crate::runtime::HttpMethod;
        assert_eq!(
            MethodIdempotency::from_method(&HttpMethod::POST),
            MethodIdempotency::NonIdempotent
        );
    }
}
