use crate::runtime::HeaderOperations;
use http::header::{HeaderName, HeaderValue};
use std::str::FromStr;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_headers(
    headers: &HeaderOperations,
    context: &str,
) -> CoreValidationResult<()> {
    for action in &headers.actions {
        if action.key.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HeaderName::from_str(&action.key).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: action.key.clone(),
        })?;

        if let Some(value) = &action.value {
            HeaderValue::from_str(value).map_err(|_| CoreValidationError::InvalidHeaderValue {
                context: context.to_string(),
                name: action.key.clone(),
            })?;
        }

        // For Remove action, value is ignored, so we don't strictly need to check it if it's present,
        // but checking it is safer/cleaner.
        // If strict validation is required:
        // if action.action == HeaderActionType::Remove && action.value.is_some() { ... }
    }

    Ok(())
}
