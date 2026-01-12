use crate::runtime::{SniName, TlsPolicy, TlsVerify, Upstream};
use std::collections::HashSet;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_upstreams(upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let mut names = HashSet::new();

    for u in upstreams {
        if u.name.0.is_empty() {
            return Err(CoreValidationError::EmptyUpstreamName);
        }
        if !names.insert(&u.name.0) {
            return Err(CoreValidationError::DuplicateUpstream(u.name.0.clone()));
        }
        if let TlsPolicy::Enabled { verify, sni, .. } = &u.tls
            && matches!(verify, TlsVerify::Full)
            && matches!(sni, SniName::Disabled)
        {
            return Err(CoreValidationError::UpstreamTlsSniDisabled(
                u.name.0.clone(),
            ));
        }
        for _ep in &u.endpoints {
            // Weight is NonZeroU16; zero is not representable in a valid runtime config.
        }
    }
    Ok(())
}
