use crate::runtime::TlsConfig;
use std::net::SocketAddr;

use super::{CoreValidationError, CoreValidationResult};

#[allow(clippy::collapsible_if)]
pub(super) const fn validate_server(
    _listen_addr: SocketAddr,
    tls: Option<&TlsConfig>,
) -> CoreValidationResult<()> {
    if let Some(tls_cfg) = tls {
        if tls_cfg.enabled {
            if tls_cfg.cert_path.is_none() || tls_cfg.key_path.is_none() {
                return Err(CoreValidationError::MissingTlsFiles);
            }
        }
    }
    Ok(())
}
