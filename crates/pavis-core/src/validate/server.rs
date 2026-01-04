use crate::runtime::TlsConfig;
use std::net::SocketAddr;

use super::{CoreValidationError, CoreValidationResult};

#[allow(clippy::collapsible_if)]
pub(super) fn validate_server(
    _listen_addr: SocketAddr,
    tls: &TlsConfig,
) -> CoreValidationResult<()> {
    if let TlsConfig::Enabled {
        cert_path,
        key_path,
    } = tls
    {
        if cert_path.0.is_empty() || key_path.0.is_empty() {
            return Err(CoreValidationError::MissingTlsFiles);
        }
    }
    Ok(())
}
