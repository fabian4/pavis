use crate::runtime::{
    HeaderName as RuntimeHeaderName, HeaderValue as RuntimeHeaderValue, HeadersPolicy,
};
use http::header::{HeaderName as HttpHeaderName, HeaderValue as HttpHeaderValue};
use std::str::FromStr;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_headers(headers: &HeadersPolicy, context: &str) -> CoreValidationResult<()> {
    let rules = match headers {
        HeadersPolicy::Disabled => return Ok(()),
        HeadersPolicy::Enabled { rules } => rules,
    };

    let validate_pair = |name: &RuntimeHeaderName, value: &RuntimeHeaderValue| {
        if name.0.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HttpHeaderName::from_str(&name.0).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.0.clone(),
        })?;
        HttpHeaderValue::from_str(&value.0).map_err(|_| {
            CoreValidationError::InvalidHeaderValue {
                context: context.to_string(),
                name: name.0.clone(),
            }
        })?;
        Ok(())
    };

    for (name, value) in &rules.set_headers {
        validate_pair(name, value)?;
    }
    for (name, value) in &rules.append_headers {
        validate_pair(name, value)?;
    }
    for (name, value) in &rules.add_headers {
        validate_pair(name, value)?;
    }
    for name in &rules.remove_headers {
        if name.0.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HttpHeaderName::from_str(&name.0).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.0.clone(),
        })?;
    }

    Ok(())
}
