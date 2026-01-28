use pavis_core::{Principal, SpiffeId};
use pingora::proxy::Session;

pub fn extract_client_identity(session: &Session) -> Option<SpiffeId> {
    let stream = session.as_downstream().stream()?;
    let ssl = stream.get_ssl()?;
    let cert = ssl.peer_certificate()?;
    let cert_der = cert.to_der().ok()?;
    crate::proxy::identity::extract_spiffe_id(&cert_der)
}

pub fn is_authorized(principal: &Principal, client_identity: Option<&SpiffeId>) -> bool {
    match principal {
        Principal::Any => true,
        Principal::Authenticated { spiffe } => {
            client_identity.is_some_and(|identity| identity.as_str() == spiffe.as_str())
        }
        Principal::Prefix { prefix } => {
            client_identity.is_some_and(|identity| identity.as_str().starts_with(prefix.as_str()))
        }
        #[allow(unreachable_patterns)]
        _ => false,
    }
}
