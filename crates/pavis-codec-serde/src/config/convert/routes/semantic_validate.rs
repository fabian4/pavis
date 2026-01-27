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
