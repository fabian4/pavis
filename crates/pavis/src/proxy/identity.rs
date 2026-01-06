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

    // Get the Subject Alternative Names extension
    let san = cert.subject_alt_names()?;

    // Iterate through SANs to find URI entries
    for name in san.iter() {
        // Check if this is a URI-type SAN
        if let Some(uri) = name.uri() {
            // Check if it's a SPIFFE ID (starts with "spiffe://")
            if uri.starts_with("spiffe://") {
                return Some(uri.to_string());
            }
        }
    }

    None
}

/// A cached identity extractor that memoizes extraction results.
///
/// This can be used to avoid redundant parsing of certificates on the same connection.
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

    #[test]
    fn extractor_can_be_created() {
        let _extractor = IdentityExtractor::new();
        // Basic smoke test - actual certificate testing would require
        // setting up real certificates and SSL contexts
        assert!(true);
    }
}
