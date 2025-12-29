use crate::runtime::HeaderOperations;
use http::header::{HeaderName, HeaderValue};
use std::str::FromStr;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_headers(
    headers: &HeaderOperations,
    context: &str,
) -> CoreValidationResult<()> {
    for (name, value) in &headers.add {
        if name.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HeaderName::from_str(name).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.clone(),
        })?;
        HeaderValue::from_str(value).map_err(|_| CoreValidationError::InvalidHeaderValue {
            context: context.to_string(),
            name: name.clone(),
        })?;
    }

    for name in &headers.remove {
        if name.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HeaderName::from_str(name).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.clone(),
        })?;
    }

    Ok(())
}
