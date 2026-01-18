# Known Limits & Constraints

## TLS Backend Limitations (Rustls)

Pavis currently uses Pingora's rustls backend, which has the following limitations due to upstream constraints:

1. **No Inbound mTLS**: Pingora's rustls listener does not expose an API to configure client certificate verification.
2. **No Per-Peer CA Verification**: Upstream TLS connections can only use the system-wide CA bundle. Custom CA certificates specified via `ca_bundle_path` are ignored by the rustls connector.

Users requiring these features should use the **OpenSSL/BoringSSL backend** (available via build-time feature flags).
