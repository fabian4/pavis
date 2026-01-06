use crate::runtime::TlsConfig;
use std::net::SocketAddr;

use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_server(
    _listen_addr: SocketAddr,
    tls: &TlsConfig,
) -> CoreValidationResult<()> {
    if let TlsConfig::Enabled {
        cert_path,
        key_path,
        client_auth,
    } = tls
    {
        if cert_path.0.is_empty() || key_path.0.is_empty() {
            return Err(CoreValidationError::MissingTlsFiles);
        }

        // Validate client auth CA paths
        match client_auth {
            crate::runtime::ClientAuth::Disabled => {}
            crate::runtime::ClientAuth::Optional { ca_path }
            | crate::runtime::ClientAuth::Required { ca_path } => {
                if ca_path.0.is_empty() {
                    return Err(CoreValidationError::MissingTlsFiles);
                }
            }
        }
    }
    Ok(())
}
