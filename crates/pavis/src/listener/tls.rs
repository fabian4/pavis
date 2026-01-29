use anyhow::{Context, Result};
use pavis_core::{ClientAuth, ListenerName, Path as RuntimePath, TlsConfig};
use pingora::listeners::tls::TlsSettings;
use pingora::tls::ssl::SslVerifyMode;

/// Materializes TLS settings for Pingora listeners.
///
/// This encapsulates OpenSSL-specific behavior (certificate loading,
/// client-auth verification flags) so the bootstrapper only decides whether a
/// listener is TCP or TLS.
pub struct TlsRuntime;

impl TlsRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn build(
        &self,
        listener_name: &ListenerName,
        tls: &TlsConfig,
    ) -> Result<Option<TlsSettings>> {
        let TlsConfig::Enabled {
            cert_path,
            key_path,
            client_auth,
        } = tls
        else {
            return Ok(None);
        };

        let mut tls_settings = TlsSettings::intermediate(&cert_path.0, &key_path.0)
            .with_context(|| format!("Failed to configure TLS for listener {}", listener_name.0))?;
        Self::apply_client_auth(&mut tls_settings, client_auth)?;
        Ok(Some(tls_settings))
    }

    fn apply_client_auth(settings: &mut TlsSettings, client_auth: &ClientAuth) -> Result<()> {
        match client_auth {
            ClientAuth::Disabled => Ok(()),
            ClientAuth::Optional { ca_path } => {
                tracing::debug!(ca_path = %ca_path.0, "Configuring optional client certificate authentication");
                configure_client_auth(settings, ca_path, false)
            }
            ClientAuth::Required { ca_path } => {
                tracing::debug!(ca_path = %ca_path.0, "Configuring required client certificate authentication");
                configure_client_auth(settings, ca_path, true)
            }
            #[allow(unreachable_patterns)]
            &_ => Ok(()),
        }
    }
}

impl Default for TlsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn configure_client_auth(
    tls_settings: &mut TlsSettings,
    ca_path: &RuntimePath,
    require_client_cert: bool,
) -> Result<()> {
    tls_settings
        .set_ca_file(&ca_path.0)
        .with_context(|| format!("Failed to load client CA bundle {}", ca_path.0))?;
    let mut verify_mode = SslVerifyMode::PEER;
    if require_client_cert {
        verify_mode |= SslVerifyMode::FAIL_IF_NO_PEER_CERT;
    }
    tls_settings.set_verify(verify_mode);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::TlsRuntime;
    use pavis_core::{ClientAuth, ListenerName, Path, TlsConfig};
    use rand::{RngCore, rng};
    use std::fs;

    fn write_pem(path: &std::path::Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write pem");
    }

    // Pure-Rust replacement for OpenSSL cert generation
    fn build_ca_cert() -> (rcgen::CertifiedIssuer<'static, rcgen::KeyPair>, String) {
        let mut params = rcgen::CertificateParams::new(vec!["Pavis Test CA".to_string()]).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Pavis Test CA");
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let issuer = rcgen::CertifiedIssuer::self_signed(params, key_pair).unwrap();
        let pem = issuer.pem();
        (issuer, pem)
    }

    fn build_server_cert(
        ca_issuer: &rcgen::CertifiedIssuer<'_, rcgen::KeyPair>,
    ) -> (String, String) {
        let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "localhost");
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.signed_by(&key_pair, ca_issuer).unwrap();
        (key_pair.serialize_pem(), cert.pem())
    }

    fn temp_dir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let mut nonce = [0u8; 8];
        let mut rng = rng();
        rng.fill_bytes(&mut nonce);
        dir.push(format!("pavis_tls_runtime_{:x}", u64::from_le_bytes(nonce)));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn tls_runtime_builds_with_optional_client_auth() {
        let dir = temp_dir();
        let ca_path = dir.join("ca.pem");
        let cert_path = dir.join("server.pem");
        let key_path = dir.join("server.key");

        let (ca_issuer, ca_cert_pem) = build_ca_cert();
        let (server_key_pem, server_cert_pem) = build_server_cert(&ca_issuer);

        write_pem(&ca_path, ca_cert_pem.as_bytes());
        write_pem(&cert_path, server_cert_pem.as_bytes());
        write_pem(&key_path, server_key_pem.as_bytes());

        let runtime = TlsRuntime::new();
        let listener_name = ListenerName("listener".to_string());
        let tls = TlsConfig::Enabled {
            cert_path: Path(cert_path.to_string_lossy().into_owned()),
            key_path: Path(key_path.to_string_lossy().into_owned()),
            client_auth: ClientAuth::Optional {
                ca_path: Path(ca_path.to_string_lossy().into_owned()),
            },
        };

        let settings = runtime
            .build(&listener_name, &tls)
            .expect("build tls settings");
        assert!(settings.is_some());
    }

    #[test]
    fn tls_runtime_builds_with_required_client_auth() {
        let dir = temp_dir();
        let ca_path = dir.join("ca.pem");
        let cert_path = dir.join("server.pem");
        let key_path = dir.join("server.key");

        let (ca_issuer, ca_cert_pem) = build_ca_cert();
        let (server_key_pem, server_cert_pem) = build_server_cert(&ca_issuer);

        write_pem(&ca_path, ca_cert_pem.as_bytes());
        write_pem(&cert_path, server_cert_pem.as_bytes());
        write_pem(&key_path, server_key_pem.as_bytes());

        let runtime = TlsRuntime::new();
        let listener_name = ListenerName("listener".to_string());
        let tls = TlsConfig::Enabled {
            cert_path: Path(cert_path.to_string_lossy().into_owned()),
            key_path: Path(key_path.to_string_lossy().into_owned()),
            client_auth: ClientAuth::Required {
                ca_path: Path(ca_path.to_string_lossy().into_owned()),
            },
        };

        let settings = runtime
            .build(&listener_name, &tls)
            .expect("build tls settings");
        assert!(settings.is_some());
    }
}
