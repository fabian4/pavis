use crate::runtime::{ActiveHealthCheck, ReuseAcrossSni, SniName, TlsPolicy, TlsVerify, Upstream};
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
        if matches!(
            &u.tls,
            TlsPolicy::Enabled {
                verify: TlsVerify::Full,
                sni: SniName::Disabled,
                ..
            }
        ) {
            return Err(CoreValidationError::UpstreamTlsSniDisabled(
                u.name.0.clone(),
            ));
        }
        if matches!(
            &u.tls,
            TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                reuse_across_sni: ReuseAcrossSni::Enabled,
                ..
            }
        ) {
            return Err(
                CoreValidationError::UpstreamTlsReuseAcrossSniRequiresVerify(u.name.0.clone()),
            );
        }
        if let ActiveHealthCheck::Enabled {
            path,
            interval,
            timeout,
        } = &u.health_check
        {
            let path_value = path.0.as_str();
            if path_value.is_empty() || !path_value.starts_with('/') || path_value.contains(' ') {
                return Err(CoreValidationError::InvalidHealthCheckPath(
                    u.name.0.clone(),
                ));
            }
            if timeout.0.get() > interval.0.get() {
                return Err(CoreValidationError::HealthCheckTimeoutExceedsInterval(
                    u.name.0.clone(),
                ));
            }
        }
        for _ep in &u.endpoints {
            // Weight is NonZeroU16; zero is not representable in a valid runtime config.
        }
    }
    Ok(())
}
