//! Identity extraction from X.509 certificates.
//!
//! This module provides functionality to extract SPIFFE identities from
//! client certificates presented during mTLS handshakes.

use pingora::tls::ssl::SslRef;

/// Extracts the SPIFFE identity from a client certificate.
///
/// This function parses the X.509 Subject Alternative Names (SANs) to find
/// URI-type SANs that represent SPIFFE IDs (e.g., "spiffe://cluster.local/ns/prod/sa/app").
///
/// # Arguments
/// * `ssl` - Reference to the SSL connection
///
/// # Returns
/// * `Some(String)` - The SPIFFE ID if found in the certificate
/// * `None` - If no certificate is present or no SPIFFE ID is found
pub fn extract_spiffe_id(ssl: &SslRef) -> Option<String> {
    // Get the peer certificate (client certificate)
    let cert = ssl.peer_certificate()?;
    extract_spiffe_id_from_cert(&cert)
}

/// Extracts the SPIFFE identity from an X.509 certificate.
pub fn extract_spiffe_id_from_cert(cert: &openssl::x509::X509) -> Option<String> {
    // Iterate through SANs to find URI entries
    if let Some(san) = cert.subject_alt_names() {
        for name in san.iter() {
            if let Some(uri) = name.uri().filter(|u| u.starts_with("spiffe://")) {
                return Some(uri.to_string());
            }
        }
    }

    None
}

/// A cached identity extractor that memoizes extraction results.
#[derive(Clone)]
pub struct IdentityExtractor {
    // Future optimization: Add a cache here if needed
    // For now, we extract on-demand since certificate parsing is relatively fast
}

impl IdentityExtractor {
    /// Creates a new identity extractor.
    pub fn new() -> Self {
        Self {}
    }

    /// Extracts the SPIFFE identity from the SSL connection.
    pub fn extract(&self, ssl: &SslRef) -> Option<String> {
        extract_spiffe_id(ssl)
    }
}

impl Default for IdentityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pingora::tls::ssl::{Ssl, SslContext, SslMethod};

    #[test]
    fn extractor_can_be_created() {
        let extractor = IdentityExtractor::new();
        let default_extractor = IdentityExtractor::default();
        let ctx = SslContext::builder(SslMethod::tls()).unwrap().build();
        let ssl = Ssl::new(&ctx).unwrap();
        assert_eq!(extractor.extract(&ssl), None);
        assert_eq!(default_extractor.extract(&ssl), None);
    }

    #[test]
    fn test_extract_spiffe_id_no_cert() {
        let ctx = SslContext::builder(SslMethod::tls()).unwrap().build();
        let ssl = Ssl::new(&ctx).unwrap();
        assert_eq!(extract_spiffe_id(&ssl), None);
    }

    #[test]
    fn test_extract_spiffe_id_with_cert() {
        use openssl::asn1::Asn1Time;
        use openssl::bn::BigNum;
        use openssl::hash::MessageDigest;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::extension::SubjectAlternativeName;
        use openssl::x509::{X509Builder, X509NameBuilder};

        // 1. Generate a key pair
        let rsa = Rsa::generate(2048).unwrap();
        let pkey = PKey::from_rsa(rsa).unwrap();

        // 2. Build a certificate with SPIFFE ID in SAN
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "test").unwrap();
        let name = name.build();

        let mut builder = X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        let serial_number = BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap();
        builder.set_serial_number(&serial_number).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        let not_before = Asn1Time::days_from_now(0).unwrap();
        builder.set_not_before(&not_before).unwrap();
        let not_after = Asn1Time::days_from_now(365).unwrap();
        builder.set_not_after(&not_after).unwrap();

        let spiffe_id = "spiffe://example.org/ns/foo/sa/bar";
        let san = SubjectAlternativeName::new()
            .uri(spiffe_id)
            .build(&builder.x509v3_context(None, None))
            .unwrap();
        builder.append_extension(san).unwrap();

        builder.sign(&pkey, MessageDigest::sha256()).unwrap();
        let cert = builder.build();

        // 3. Test extraction from cert
        assert_eq!(
            extract_spiffe_id_from_cert(&cert),
            Some(spiffe_id.to_string())
        );

        // 4. Test with non-spiffe URI
        let mut builder = X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        builder.set_serial_number(&serial_number).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder.set_not_before(&not_before).unwrap();
        builder.set_not_after(&not_after).unwrap();
        let san = SubjectAlternativeName::new()
            .uri("https://not-spiffe.com")
            .build(&builder.x509v3_context(None, None))
            .unwrap();
        builder.append_extension(san).unwrap();
        builder.sign(&pkey, MessageDigest::sha256()).unwrap();
        let cert = builder.build();
        assert_eq!(extract_spiffe_id_from_cert(&cert), None);
    }
}
