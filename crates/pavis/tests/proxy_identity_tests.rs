//! Comprehensive tests for proxy/identity.rs
//!
//! This test file covers SPIFFE identity extraction from X.509 certificates,
//! including all URI parsing edge cases, certificate validation, and error paths.

use pavis::proxy::identity::{IdentityExtractor, extract_spiffe_id, extract_spiffe_id_from_cert};
use pavis_core::SpiffeId;
use rcgen::{CertificateParams, KeyPair, SanType};
use x509_parser::prelude::*;

fn build_cert_der(uris: &[&str]) -> Vec<u8> {
    let mut params = CertificateParams::new(vec!["test.example.com".to_string()]).unwrap();
    for uri in uris {
        let ia5 = rcgen::string::Ia5String::try_from(*uri).unwrap();
        params.subject_alt_names.push(SanType::URI(ia5));
    }
    let key_pair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    cert.der().to_vec()
}

fn build_cert_without_san() -> Vec<u8> {
    let params = CertificateParams::new(vec!["test.example.com".to_string()]).unwrap();
    let key_pair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    cert.der().to_vec()
}

#[test]
fn test_extract_spiffe_id_valid_simple() {
    let cert = build_cert_der(&["spiffe://example.org/ns/default/sa/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://example.org/ns/default/sa/service".to_string()
        ))
    );
}

#[test]
fn test_extract_spiffe_id_valid_complex_path() {
    let cert = build_cert_der(&["spiffe://trust-domain.com/very/deep/nested/path/to/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://trust-domain.com/very/deep/nested/path/to/service".to_string()
        ))
    );
}

#[test]
fn test_extract_spiffe_id_no_san_extension() {
    let cert = build_cert_without_san();
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_non_spiffe_uri() {
    let cert = build_cert_der(&["https://example.com"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_http_uri() {
    let cert = build_cert_der(&["http://not-spiffe.org/path"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_multiple_non_spiffe_uris() {
    let cert = build_cert_der(&["https://example.com", "http://test.org"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_spiffe_with_other_uris() {
    let cert = build_cert_der(&["https://example.com", "spiffe://trust.org/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_multiple_spiffe_uris_rejected() {
    // Multiple SPIFFE IDs should be rejected (strict mode)
    let cert = build_cert_der(&[
        "spiffe://trust1.org/service1",
        "spiffe://trust2.org/service2",
    ]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_case_insensitive_scheme() {
    let cert = build_cert_der(&["SPIFFE://example.org/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://example.org/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_mixed_case_scheme() {
    let cert = build_cert_der(&["SpIfFe://example.org/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://example.org/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_invalid_der() {
    let invalid_der = vec![0xFF, 0xFE, 0xFD, 0xFC];
    let result = extract_spiffe_id(&invalid_der);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_empty_der() {
    let result = extract_spiffe_id(&[]);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_from_cert_parsed() {
    let cert_der = build_cert_der(&["spiffe://example.org/workload"]);
    let (_, cert) = X509Certificate::from_der(&cert_der).unwrap();
    let result = extract_spiffe_id_from_cert(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://example.org/workload".to_string()))
    );
}

#[test]
fn test_parse_spiffe_uri_valid_simple() {
    let cert = build_cert_der(&["spiffe://trust.org/path"]);
    let result = extract_spiffe_id(&cert);

    assert!(result.is_some());
}

#[test]
fn test_parse_spiffe_uri_missing_slash() {
    // "spiffe://trust.org" without path should be rejected
    let cert = build_cert_der(&["spiffe://trust.org"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_parse_spiffe_uri_only_slashes_in_path() {
    // Path with only slashes should be rejected
    let cert = build_cert_der(&["spiffe://trust.org///"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_parse_spiffe_uri_empty_trust_domain() {
    // Empty trust domain should be rejected
    let cert = build_cert_der(&["spiffe:///path"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_parse_spiffe_uri_empty_path() {
    // Empty path (just slash) should be rejected
    let cert = build_cert_der(&["spiffe://trust.org/"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_parse_spiffe_uri_no_scheme_separator() {
    let cert = build_cert_der(&["spiffe-trust.org/path"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_parse_spiffe_uri_with_port() {
    let cert = build_cert_der(&["spiffe://trust.org:8080/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org:8080/service".to_string()))
    );
}

#[test]
fn test_parse_spiffe_uri_with_special_chars_in_path() {
    let cert = build_cert_der(&["spiffe://trust.org/ns-1/service_2"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org/ns-1/service_2".to_string()))
    );
}

#[test]
fn test_parse_spiffe_uri_with_subdomain() {
    let cert = build_cert_der(&["spiffe://prod.trust.example.com/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://prod.trust.example.com/service".to_string()
        ))
    );
}

#[test]
fn test_identity_extractor_new() {
    let extractor = IdentityExtractor::new();
    let cert = build_cert_der(&["spiffe://example.org/service"]);
    let result = extractor.extract(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://example.org/service".to_string()))
    );
}

#[test]
fn test_identity_extractor_default() {
    let extractor = IdentityExtractor::default();
    let cert = build_cert_der(&["spiffe://example.org/default-service"]);
    let result = extractor.extract(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://example.org/default-service".to_string()))
    );
}

#[test]
fn test_identity_extractor_clone() {
    let extractor = IdentityExtractor::new();
    let cloned = extractor.clone();
    let cert = build_cert_der(&["spiffe://example.org/cloned"]);

    let result1 = extractor.extract(&cert);
    let result2 = cloned.extract(&cert);

    assert_eq!(result1, result2);
    assert_eq!(
        result1,
        Some(SpiffeId("spiffe://example.org/cloned".to_string()))
    );
}

#[test]
fn test_identity_extractor_no_spiffe_id() {
    let extractor = IdentityExtractor::new();
    let cert = build_cert_der(&["https://example.com"]);
    let result = extractor.extract(&cert);

    assert_eq!(result, None);
}

#[test]
fn test_identity_extractor_invalid_cert() {
    let extractor = IdentityExtractor::new();
    let result = extractor.extract(&[0x00, 0x01, 0x02]);

    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_single_slash_after_domain() {
    let cert = build_cert_der(&["spiffe://trust.org/s"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, Some(SpiffeId("spiffe://trust.org/s".to_string())));
}

#[test]
fn test_extract_spiffe_id_long_trust_domain() {
    let cert = build_cert_der(&["spiffe://very.long.trust.domain.example.com/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://very.long.trust.domain.example.com/service".to_string()
        ))
    );
}

#[test]
fn test_extract_spiffe_id_numeric_trust_domain() {
    let cert = build_cert_der(&["spiffe://192.168.1.1/service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://192.168.1.1/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_path_with_numbers() {
    let cert = build_cert_der(&["spiffe://trust.org/123/456/789"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org/123/456/789".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_multiple_mixed_uris() {
    let cert = build_cert_der(&[
        "https://example.com",
        "http://test.org",
        "spiffe://trust.org/service",
        "ftp://files.net",
    ]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_consistency() {
    // Multiple extractions should return the same result
    let cert = build_cert_der(&["spiffe://example.org/consistent"]);

    let result1 = extract_spiffe_id(&cert);
    let result2 = extract_spiffe_id(&cert);
    let result3 = extract_spiffe_id(&cert);

    assert_eq!(result1, result2);
    assert_eq!(result2, result3);
}

#[test]
fn test_identity_extractor_multiple_extractions() {
    let extractor = IdentityExtractor::new();
    let cert1 = build_cert_der(&["spiffe://example.org/service1"]);
    let cert2 = build_cert_der(&["spiffe://example.org/service2"]);

    let result1 = extractor.extract(&cert1);
    let result2 = extractor.extract(&cert2);

    assert_eq!(
        result1,
        Some(SpiffeId("spiffe://example.org/service1".to_string()))
    );
    assert_eq!(
        result2,
        Some(SpiffeId("spiffe://example.org/service2".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_rejects_two_spiffe_ids_different_domains() {
    let cert = build_cert_der(&[
        "spiffe://trust1.example.com/service",
        "spiffe://trust2.example.com/service",
    ]);
    let result = extract_spiffe_id(&cert);

    // Strict mode: multiple SPIFFE IDs rejected
    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_rejects_two_spiffe_ids_same_domain() {
    let cert = build_cert_der(&["spiffe://trust.org/service1", "spiffe://trust.org/service2"]);
    let result = extract_spiffe_id(&cert);

    // Strict mode: multiple SPIFFE IDs rejected
    assert_eq!(result, None);
}

#[test]
fn test_extract_spiffe_id_first_uri_non_spiffe_second_spiffe() {
    let cert = build_cert_der(&["https://example.com", "spiffe://trust.org/service"]);
    let result = extract_spiffe_id(&cert);

    // Should find the SPIFFE ID
    assert_eq!(
        result,
        Some(SpiffeId("spiffe://trust.org/service".to_string()))
    );
}

#[test]
fn test_extract_spiffe_id_with_hyphen_in_path() {
    let cert = build_cert_der(&["spiffe://trust.org/my-namespace/my-service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://trust.org/my-namespace/my-service".to_string()
        ))
    );
}

#[test]
fn test_extract_spiffe_id_with_underscore_in_path() {
    let cert = build_cert_der(&["spiffe://trust.org/my_namespace/my_service"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(
        result,
        Some(SpiffeId(
            "spiffe://trust.org/my_namespace/my_service".to_string()
        ))
    );
}

#[test]
fn test_extract_spiffe_id_minimal_valid() {
    // Minimal valid SPIFFE ID: domain + single char path
    let cert = build_cert_der(&["spiffe://t.o/a"]);
    let result = extract_spiffe_id(&cert);

    assert_eq!(result, Some(SpiffeId("spiffe://t.o/a".to_string())));
}
