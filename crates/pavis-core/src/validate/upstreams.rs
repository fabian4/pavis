use crate::runtime::Upstream;
use std::collections::HashSet;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_upstreams(upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let mut names = HashSet::new();

    for u in upstreams {
        if u.name.is_empty() {
            return Err(CoreValidationError::EmptyUpstreamName);
        }
        if !names.insert(&u.name) {
            return Err(CoreValidationError::DuplicateUpstream(u.name.clone()));
        }
        for ep in &u.endpoints {
            if ep.weight == 0 {
                return Err(CoreValidationError::EndpointWeightZero(u.name.clone()));
            }
        }
    }
    Ok(())
}
