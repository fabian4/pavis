use crate::config::types::{
    BackoffStrategyDTO, HeaderMatcherDTO, HeaderPredicate as CodecHeaderPredicate,
    HeaderPredicateLegacy, RetryPolicy,
};
use anyhow::Result;
use compact_str::CompactString;
use pavis_core::{
    BackoffStrategy, Duration, ErrorCode, FieldPathBuilder, HeaderMatch, HeaderPredicate,
    HttpMethod, PavisError, RetryPolicy as CoreRetryPolicy, RetryReason, RetryableStatusCodes,
    Timeout, TryTimeout,
};
use std::num::NonZeroU16;

pub fn parse_http_method(method: &str, field_path: String) -> Result<HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "DELETE" => Ok(HttpMethod::DELETE),
        "PATCH" => Ok(HttpMethod::PATCH),
        "HEAD" => Ok(HttpMethod::HEAD),
        "OPTIONS" => Ok(HttpMethod::OPTIONS),
        "CONNECT" => Ok(HttpMethod::CONNECT),
        "TRACE" => Ok(HttpMethod::TRACE),
        _ => Err(invalid_config_error(
            format!(
                "invalid HTTP method '{}' (supported: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS, CONNECT, TRACE)",
                method
            ),
            Some(field_path),
            Some("valid_http_method"),
        )),
    }
}

pub fn to_runtime_header_predicate(
    pred: CodecHeaderPredicate,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<HeaderPredicate> {
    match pred {
        CodecHeaderPredicate::V1(legacy) => {
            to_runtime_header_predicate_legacy(legacy, vhost_index, path_index, header_index)
        }
        CodecHeaderPredicate::V2(dto) => {
            to_runtime_header_predicate_dto(dto, vhost_index, path_index, header_index)
        }
    }
}

fn to_runtime_header_predicate_dto(
    pred: HeaderMatcherDTO,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<HeaderPredicate> {
    let (name, matcher) = match pred {
        HeaderMatcherDTO::Exact { name, value } => {
            (name, HeaderMatch::Exact(CompactString::new(value)))
        }
        HeaderMatcherDTO::Prefix { name, prefix } => {
            (name, HeaderMatch::Prefix(CompactString::new(prefix)))
        }
        HeaderMatcherDTO::Regex { name, pattern } => {
            let header_path = header_field_path(vhost_index, path_index, Some(&name), header_index);
            if pattern.len() > 256 {
                return Err(invalid_config_error(
                    format!("regex pattern for header '{}' exceeds 256 bytes", name),
                    Some(header_path.clone()),
                    Some("regex_pattern_too_long"),
                ));
            }
            regex::Regex::new(&pattern).map_err(|e| {
                invalid_config_error(
                    format!("invalid regex pattern for header '{}': {}", name, e),
                    Some(header_path),
                    Some("regex_invalid_syntax"),
                )
            })?;
            (name, HeaderMatch::Regex(CompactString::new(pattern)))
        }
        HeaderMatcherDTO::Present { name } => (name, HeaderMatch::Present),
        HeaderMatcherDTO::Absent { name } => (name, HeaderMatch::Absent),
    };

    validate_header_name(&name, vhost_index, path_index, header_index)?;

    Ok(HeaderPredicate {
        name: CompactString::new(name.to_ascii_lowercase()),
        matcher,
    })
}

pub fn validate_header_name(
    name: &str,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<()> {
    if name.is_empty() {
        return Err(invalid_config_error(
            "header predicate name cannot be empty",
            Some(header_field_path(
                vhost_index,
                path_index,
                None,
                header_index,
            )),
            Some("header_name_non_empty"),
        ));
    }
    if !name
        .chars()
        .all(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(invalid_config_error(
            format!(
                "invalid header name '{}' (must contain only alphanumeric, -, or _)",
                name
            ),
            Some(header_field_path(
                vhost_index,
                path_index,
                Some(name),
                header_index,
            )),
            Some("header_name_invalid"),
        ));
    }
    Ok(())
}

fn to_runtime_header_predicate_legacy(
    pred: HeaderPredicateLegacy,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<HeaderPredicate> {
    let canonical_name = pred.name.to_ascii_lowercase();
    let header_path =
        header_field_path(vhost_index, path_index, Some(&canonical_name), header_index);

    validate_header_name(&pred.name, vhost_index, path_index, header_index)?;

    let matcher = if pred.absent {
        if pred.value.is_some() || pred.regex || pred.prefix {
            return Err(invalid_config_error(
                format!(
                    "header predicate '{}': absent=true is incompatible with value, regex, or prefix",
                    pred.name
                ),
                Some(header_path.clone()),
                Some("header_absent_incompatible"),
            ));
        }
        HeaderMatch::Absent
    } else {
        match (&pred.value, pred.regex, pred.prefix) {
            (None, false, false) => HeaderMatch::Present,
            (Some(val), false, false) => HeaderMatch::Exact(CompactString::new(val)),
            (Some(prefix_val), false, true) => HeaderMatch::Prefix(CompactString::new(prefix_val)),
            (Some(pattern), true, false) => {
                if pattern.len() > 256 {
                    return Err(invalid_config_error(
                        format!("regex pattern for header '{}' exceeds 256 bytes", pred.name),
                        Some(header_path.clone()),
                        Some("regex_pattern_too_long"),
                    ));
                }
                regex::Regex::new(pattern).map_err(|e| {
                    invalid_config_error(
                        format!("invalid regex pattern for header '{}': {}", pred.name, e),
                        Some(header_path.clone()),
                        Some("regex_invalid_syntax"),
                    )
                })?;
                HeaderMatch::Regex(CompactString::new(pattern))
            }
            (None, true, _) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': regex=true requires a value",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("regex_requires_value"),
                ));
            }
            (None, _, true) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': prefix=true requires a value",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("prefix_requires_value"),
                ));
            }
            (Some(_), true, true) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': regex and prefix are mutually exclusive",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("regex_prefix_exclusive"),
                ));
            }
        }
    };

    Ok(HeaderPredicate {
        name: CompactString::new(&canonical_name),
        matcher,
    })
}

pub fn convert_retry_policy(
    dto: RetryPolicy,
    route_timeout: &Timeout,
    vh_index: usize,
    path_index: usize,
) -> Result<CoreRetryPolicy> {
    let field_path_base = format!("routes[{}].paths[{}].retry", vh_index, path_index);

    if dto.max_attempts == 0 {
        return Err(invalid_config_error(
            "max_attempts must be >= 1",
            Some(format!("{}.max_attempts", field_path_base)),
            Some("min_value=1"),
        ));
    }
    if dto.max_attempts > 10 {
        return Err(invalid_config_error(
            format!("max_attempts {} exceeds maximum of 10", dto.max_attempts),
            Some(format!("{}.max_attempts", field_path_base)),
            Some("max_value=10"),
        ));
    }

    let max_attempts = NonZeroU16::new(dto.max_attempts)
        .ok_or_else(|| anyhow::anyhow!("max_attempts must be > 0"))?;

    let mut retryable_reasons = Vec::new();
    for reason_str in &dto.retryable_reasons {
        let reason = match reason_str.as_str() {
            "status_code" => RetryReason::StatusCode,
            "connect_timeout" => RetryReason::ConnectTimeout,
            "read_timeout" => RetryReason::ReadTimeout,
            "per_try_timeout" => RetryReason::PerTryTimeout,
            "pool_full" => RetryReason::PoolFull,
            "connect_error" => RetryReason::ConnectError,
            unknown => {
                return Err(invalid_config_error(
                    format!("unknown retryable reason: '{}'", unknown),
                    Some(format!("{}.retryable_reasons", field_path_base)),
                    Some("valid_retry_reason"),
                ));
            }
        };
        retryable_reasons.push(reason);
    }

    let has_status_code_reason = retryable_reasons
        .iter()
        .any(|r| matches!(r, RetryReason::StatusCode));

    let retryable_status_codes = if has_status_code_reason {
        match dto.retryable_status_codes {
            None => {
                return Err(invalid_config_error(
                    "retryable_status_codes is required when 'status_code' is in retryable_reasons",
                    Some(format!("{}.retryable_status_codes", field_path_base)),
                    Some("required_when_status_code_retryable"),
                ));
            }
            Some(codes) if codes.is_empty() => {
                return Err(invalid_config_error(
                    "retryable_status_codes cannot be empty when 'status_code' is in retryable_reasons",
                    Some(format!("{}.retryable_status_codes", field_path_base)),
                    Some("required_when_status_code_retryable"),
                ));
            }
            Some(codes) => Some(RetryableStatusCodes { codes }),
        }
    } else {
        dto.retryable_status_codes
            .map(|codes| RetryableStatusCodes { codes })
    };

    let backoff = match dto.backoff {
        BackoffStrategyDTO::Fixed { base_ms } => BackoffStrategy::Fixed { base_ms },
        BackoffStrategyDTO::Linear { base_ms } => BackoffStrategy::Linear { base_ms },
        BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
            BackoffStrategy::Exponential { base_ms, max_ms }
        }
    };

    let per_try = match dto.per_try {
        Some(d) => {
            let per_try_ms = d.as_millis() as u32;
            let request_timeout_ms = match route_timeout {
                Timeout::Enabled(d) => d.0.get(),
                Timeout::Disabled => 60000,
                #[allow(unreachable_patterns)]
                _ => 60000,
            };

            if per_try_ms > request_timeout_ms {
                return Err(invalid_config_error(
                    format!(
                        "per_try timeout ({}ms) exceeds overall route timeout ({}ms)",
                        per_try_ms, request_timeout_ms
                    ),
                    Some(format!("{}.per_try", field_path_base)),
                    Some("per_try_timeout_lte_request_timeout"),
                ));
            }

            TryTimeout::Enabled(Duration(
                std::num::NonZeroU32::new(per_try_ms).unwrap_or(std::num::NonZeroU32::MIN),
            ))
        }
        None => TryTimeout::Inherit,
    };

    Ok(CoreRetryPolicy::Enabled {
        max_attempts,
        per_try,
        retryable_reasons,
        retryable_status_codes,
        backoff,
        retry_non_idempotent: dto.retry_non_idempotent,
        fail_on_non_replayable_retry: dto.fail_on_non_replayable_retry,
        max_request_body_buffer_bytes: dto.max_request_body_buffer_bytes,
    })
}

fn invalid_config_error(
    message: impl Into<String>,
    field_path: Option<String>,
    constraint: Option<&str>,
) -> anyhow::Error {
    let err = PavisError::new(ErrorCode::InvalidConfig, message);
    let err = err.with_context(|ctx| {
        let mut ctx = ctx;
        if let Some(path) = field_path {
            ctx = ctx.with_field_path(path);
        }
        if let Some(code) = constraint {
            ctx = ctx.with_constraint(code.to_string());
        }
        ctx
    });
    anyhow::Error::new(err)
}

fn route_match_field_path(vhost_index: usize, path_index: usize) -> FieldPathBuilder {
    FieldPathBuilder::new()
        .root("routes")
        .index(vhost_index)
        .field("paths")
        .index(path_index)
        .field("match")
}

fn route_headers_field_path(vhost_index: usize, path_index: usize) -> FieldPathBuilder {
    route_match_field_path(vhost_index, path_index).field("headers")
}

fn header_field_path(
    vhost_index: usize,
    path_index: usize,
    header_name: Option<&str>,
    header_index: usize,
) -> String {
    let builder = route_headers_field_path(vhost_index, path_index);
    match header_name {
        Some(name) if !name.is_empty() => builder.map_key(name).finish(),
        _ => builder.index(header_index).finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{
        BackoffStrategyDTO, HeaderMatcherDTO, HeaderPredicateLegacy, RetryPolicy,
    };
    use pavis_core::Timeout;

    #[test]
    fn test_parse_http_method_valid() {
        assert!(parse_http_method("GET", "test".to_string()).is_ok());
        assert!(parse_http_method("post", "test".to_string()).is_ok());
        assert!(parse_http_method("PUT", "test".to_string()).is_ok());
    }

    #[test]
    fn test_parse_http_method_invalid() {
        let result = parse_http_method("INVALID", "test".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid HTTP method"));
    }

    #[test]
    fn test_validate_header_name_empty() {
        let result = validate_header_name("", 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_header_name_invalid_characters() {
        let result = validate_header_name("x-header!", 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid header name"));
    }

    #[test]
    fn test_validate_header_name_valid() {
        assert!(validate_header_name("x-my-header", 0, 0, 0).is_ok());
        assert!(validate_header_name("content-type", 0, 0, 0).is_ok());
        assert!(validate_header_name("X_Custom_Header", 0, 0, 0).is_ok());
    }

    #[test]
    fn test_header_predicate_dto_regex_too_long() {
        let long_pattern = "a".repeat(257);
        let dto = HeaderMatcherDTO::Regex {
            name: "x-test".to_string(),
            pattern: long_pattern,
        };

        let result = to_runtime_header_predicate_dto(dto, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds 256 bytes"));
    }

    #[test]
    fn test_header_predicate_dto_regex_invalid_syntax() {
        let dto = HeaderMatcherDTO::Regex {
            name: "x-test".to_string(),
            pattern: "[invalid".to_string(),
        };

        let result = to_runtime_header_predicate_dto(dto, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid regex pattern"));
    }

    #[test]
    fn test_header_predicate_dto_regex_valid() {
        let dto = HeaderMatcherDTO::Regex {
            name: "x-version".to_string(),
            pattern: "^v[0-9]+$".to_string(),
        };

        let result = to_runtime_header_predicate_dto(dto, 0, 0, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_header_predicate_legacy_absent_with_value_conflict() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: Some("test".to_string()),
            regex: false,
            prefix: false,
            absent: true,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("absent=true is incompatible"));
    }

    #[test]
    fn test_header_predicate_legacy_regex_without_value() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: None,
            regex: true,
            prefix: false,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("regex=true requires a value"));
    }

    #[test]
    fn test_header_predicate_legacy_prefix_without_value() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: None,
            regex: false,
            prefix: true,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("prefix=true requires a value"));
    }

    #[test]
    fn test_header_predicate_legacy_regex_and_prefix_exclusive() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: Some("test".to_string()),
            regex: true,
            prefix: true,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn test_header_predicate_legacy_regex_too_long() {
        let long_pattern = "a".repeat(257);
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: Some(long_pattern),
            regex: true,
            prefix: false,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds 256 bytes"));
    }

    #[test]
    fn test_header_predicate_legacy_regex_invalid_syntax() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: Some("[invalid".to_string()),
            regex: true,
            prefix: false,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid regex pattern"));
    }

    #[test]
    fn test_convert_retry_policy_max_attempts_zero() {
        let dto = RetryPolicy {
            max_attempts: 0,
            per_try: None,
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("must be >= 1"));
    }

    #[test]
    fn test_convert_retry_policy_max_attempts_exceeds_limit() {
        let dto = RetryPolicy {
            max_attempts: 11,
            per_try: None,
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds maximum of 10"));
    }

    #[test]
    fn test_convert_retry_policy_unknown_reason() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: None,
            retryable_reasons: vec!["unknown_reason".to_string()],
            retryable_status_codes: None,
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown retryable reason"));
    }

    #[test]
    fn test_convert_retry_policy_status_code_missing_codes() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: None,
            retryable_reasons: vec!["status_code".to_string()],
            retryable_status_codes: None,
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("retryable_status_codes is required")
        );
    }

    #[test]
    fn test_convert_retry_policy_status_code_empty_codes() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: None,
            retryable_reasons: vec!["status_code".to_string()],
            retryable_status_codes: Some(vec![]),
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_convert_retry_policy_valid() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: None,
            retryable_reasons: vec!["status_code".to_string()],
            retryable_status_codes: Some(vec![502, 503, 504]),
            backoff: BackoffStrategyDTO::Exponential {
                base_ms: 100,
                max_ms: 5000,
            },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_header_predicate_legacy_present() {
        let pred = HeaderPredicateLegacy {
            name: "x-test".to_string(),
            value: None,
            regex: false,
            prefix: false,
            absent: false,
        };

        let result = to_runtime_header_predicate_legacy(pred, 0, 0, 0);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().matcher, HeaderMatch::Present));
    }

    #[test]
    fn test_convert_retry_policy_per_try_timeout_exceeds() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: Some(std::time::Duration::from_secs(10)),
            retryable_reasons: vec![],
            retryable_status_codes: None,
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        // per_try 10s > route timeout 5s
        let result = convert_retry_policy(
            dto,
            &Timeout::Enabled(pavis_core::Duration(5000.try_into().unwrap())),
            0,
            0,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds overall route timeout"));
    }

    #[test]
    fn test_convert_retry_policy_no_status_code_reason() {
        let dto = RetryPolicy {
            max_attempts: 3,
            per_try: None,
            retryable_reasons: vec!["connect_timeout".to_string()],
            retryable_status_codes: Some(vec![503]),
            backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        };

        let result = convert_retry_policy(dto, &Timeout::Disabled, 0, 0).unwrap();
        if let CoreRetryPolicy::Enabled {
            retryable_status_codes,
            ..
        } = result
        {
            assert_eq!(retryable_status_codes.unwrap().codes, vec![503]);
        } else {
            panic!("Expected Enabled policy");
        }
    }
}
