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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_authorized() {
        let id = SpiffeId("spiffe://td/ns/svc".to_string());

        assert!(is_authorized(&Principal::Any, Some(&id)));
        assert!(is_authorized(&Principal::Any, None));

        let principal_auth = Principal::Authenticated { spiffe: id.clone() };
        assert!(is_authorized(&principal_auth, Some(&id)));
        assert!(!is_authorized(
            &principal_auth,
            Some(&SpiffeId("other".to_string()))
        ));
        assert!(!is_authorized(&principal_auth, None));

        let principal_prefix = Principal::Prefix {
            prefix: "spiffe://td".to_string(),
        };
        assert!(is_authorized(&principal_prefix, Some(&id)));
        assert!(!is_authorized(
            &principal_prefix,
            Some(&SpiffeId("spiffe://other".to_string()))
        ));
        assert!(!is_authorized(&principal_prefix, None));
    }
}
