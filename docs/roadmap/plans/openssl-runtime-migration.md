# OpenSSL Migration Plan – Pavis Runtime

_Last updated: 2026-01-27_

## 1. Discovery

### 1.1 Inbound TLS termination
- **File**: `crates/pavis/src/main.rs`
  - Functions `configure_client_auth()` and `create_root_store()` wire Pingora listeners using **rustls** primitives (`rustls::RootCertStore`, `rustls_pemfile`, `rustls::server::WebPkiClientVerifier`).
  - TODO comment notes Pingora rustls lacks client-cert verifier hooks.

### 1.2 Inbound mTLS
- Same file/function: `configure_client_auth()` handles `pavis_core::ClientAuth::{Optional,Required}` but can’t enforce mTLS because rustls connector exposes no hooks.

### 1.3 Outbound TLS (upstreams)
- **Files**: `crates/pavis/src/upstream.rs`, `crates/pavis/src/upstream/cluster.rs`
  - Use `pingora::protocols::tls::CaType` and `pingora::utils::tls::CertKey` along with `rustls_pemfile` to parse CA bundles/client certs. Per-upstream CA support is blocked by rustls backend.

### 1.4 TLS backend selection
- `crates/pavis/Cargo.toml`: `pingora` declared with `features = ["proxy", "rustls"]`; runtime also depends directly on `rustls`, `rustls-pemfile`, `webpki-roots`. `reqwest` uses rustls backend.

### 1.5 TLS fixtures/tests
- TLS/mTLS suites in `tests/suites/pavis/70_security_tls.sh`, `71_security_inbound_mtls.sh`, `74_security_mtls_outbound.sh`, `75_security_tls_sni_auto.sh`, `76_security_mtls_chain_mode.sh` currently skip with “Pingora rustls does not support per-peer CA certificates.” Scripts generate certs via `openssl` CLI and expect runtime support once backend switches.

### 1.6 Pingora OpenSSL Integration Surface (Hard Gate)
Before changing any runtime code, the developer **must**:
1. Inspect the exact Pingora version pinned in `crates/pavis/Cargo.toml` / `Cargo.lock`.
2. Enumerate the **real** OpenSSL listener builder types/functions exposed by that version (server side). Record precise Rust type names, constructor/builder functions, and required parameters (cert path, key path, CA path, client-chain path, verify mode toggles).
3. Enumerate the **real** OpenSSL client/connector types/functions used for upstream TLS (same level of detail as above).
4. Output the findings directly in this document (or a linked note) and then update §§3.1–3.2 to reference those concrete APIs.

**Hard gate**: _Do not proceed to runtime wiring (Sections 3.1/3.2) until this integration surface is nailed down and the sections are updated accordingly._

### 1.7 RuntimeConfig TLS Schema Completeness Audit (Hard Gate)
Before wiring OpenSSL, inspect `crates/pavis-core/src/runtime/**/*.rs` (especially listener/upstream TLS structs) and document whether each required semantic is represented unambiguously in `RuntimeConfig`. Use the table below and fill in actual field names/decisions:

| TLS Semantic | RuntimeConfig Field(s) | Adequate? (Yes/No) | Required Change |
| --- | --- | --- | --- |
| Listener TLS cert/key path | `runtime::server::TlsConfig::Enabled { cert_path, key_path, .. }` | Yes | None |
| Listener client-auth mode (disabled/optional/required) | `runtime::server::ClientAuth` enum | Yes | None |
| Listener client CA bundle path | `ClientAuth::{Optional,Required} { ca_path }` | Yes | None |
| Upstream CA bundle (system/file) | `runtime::upstream::UpstreamCa::{System, File { path }}` | Yes | None |
| Upstream client cert/key path | `runtime::upstream::ClientCert::Enabled { cert_path, key_path, .. }` | Yes | None |
| Upstream client chain handling | `runtime::upstream::ClientCertChain::{None, Embedded, File { path }}` | Yes | None |
| Outbound SNI override / server_name | `runtime::upstream::TlsPolicy::Enabled { sni: SniName, canonical_sni: CanonicalSni }` | Yes | None |
| Verify on/off toggles (inbound/outbound) | Inbound: `ClientAuth`; Outbound: `TlsPolicy::Enabled { verify: TlsVerify }` | Yes | None |
| Chain mode semantics | Same as `ClientCertChain` plus `reuse_across_sni` | Yes | None |
| SPIFFE extraction preconditions | `runtime::routing::Principal::Authenticated { spiffe }` combined with `ClientAuth` to supply peer cert | Yes (requires runtime to surface SPIFFE when mTLS enabled) | None |

**Hard gate**: _Do not proceed to runtime wiring until the table is filled with actual field names and any schema gaps are explicitly planned/approved. If fields are missing or ambiguous, design the schema change first._

## 2. Build & Dependency Plan (OpenSSL)

### 2.1 Cargo changes
1. `crates/pavis/Cargo.toml`
   - Hard-code `pingora` with `features = ["proxy", "openssl"]` (drop `rustls`) so every `cargo build -p pavis` and `make binary-build` emits an OpenSSL-backed binary. No optional feature flags may re-enable rustls.
   - Remove direct deps on `rustls`, `rustls-pemfile`, `webpki-roots`.
   - **Do not add `openssl` / `openssl-sys` crates by default.** First prove (via §1.6) that Pingora’s OpenSSL surface cannot express the needed semantics. Only then, as a documented last resort, add direct bindings and justify them in the PR.
   - **Reqwest change is optional/deferred**: keep `reqwest` on rustls unless a dedicated follow-up documents native-tls implications (Linux/macOS/Windows). Note this constraint in the PR description if not switching.
   - If PEM parsing/helpers are still required after leveraging Pingora, add lightweight crates (e.g., `pem = "1"`) rather than diving straight into OpenSSL X509 APIs.
   - Explicit contract: _It must be impossible to accidentally produce a rustls-backed runtime binary after this migration._
2. Ensure no other workspace crate unconditionally depends on rustls (audit with `rg -n "rustls" crates`). Document follow-ups if other binaries still need migration.

### 2.2 System dependencies (CI/dev)
- Scan **all** workflows under `.github/workflows/` (currently `pipeline.yaml`, plus any future additions such as nightly/e2e-only flows). In every job that builds Rust code or container images, add a step after checkout to install OpenSSL headers:
  ```yaml
  - name: Install OpenSSL build deps
    run: sudo apt-get update && sudo apt-get install -y libssl-dev pkg-config
  ```
  Document which files/jobs were touched in the PR description. CI must never build a rustls-backed binary.
- Docker builds (invoked via `make docker-build …`):
  - Enumerate every Dockerfile/base image (multi-stage or otherwise). Install `libssl-dev`/`pkg-config` **only in build stages**. Final runtime stages must contain runtime libraries only (e.g., `libssl3`, `openssl`), not headers.
  - Note explicitly which stage installs headers and verify they are absent from the final stage. **Do not ship OpenSSL headers or pkg-config in runtime images**; this increases size and attack surface.
- Document requirement in developer setup docs (e.g., `README.md`, `docs/BUILDING.md`).

### 2.3 Canonical build command
- `cargo build -p pavis` and `make binary-build CRATE=pavis` MUST always produce an OpenSSL-enabled runtime without additional environment variables or features. No secondary profile/build path may emit a rustls binary.
- Keep `make binary-build CRATE=pavis` as the blessed entry point (already used in CI’s `build` job) and verify `make binary-build CRATE=workspace` builds successfully on OpenSSL.

## 3. Runtime Wiring Plan (No code yet)

### 3.1 Server TLS/mTLS
- **File**: `crates/pavis/src/main.rs`
  - Rework `configure_client_auth()` to use the concrete Pingora OpenSSL listener builder(s) identified in §1.6. Feed cert/key/CA/chain paths from `pavis_core::TlsConfig` into those builders exactly as required.
  - When `ClientAuth::Required/Optional`, rely on Pingora-provided verify/CA configuration APIs; only drop to raw OpenSSL types if Pingora lacks the necessary knobs.
- Verify via TLS e2e test once unskipped.

### 3.2 Upstream TLS connectors
- **Files**: `crates/pavis/src/upstream.rs`, `crates/pavis/src/upstream/cluster.rs`
  - First attempt to reuse Pingora utilities such as `pingora::utils::tls::CertKey` and `pingora::protocols::tls::CaType` (or their OpenSSL equivalents) to load client certs, keys, chains, and CA bundles.
  - Only introduce direct OpenSSL PEM/X509 parsing if §1.6 proves Pingora’s OpenSSL backend cannot express per-upstream CA bundles, client certificate/chain loading, or verify hooks. Any such gap must be called out in the PR description before adding `openssl`/`openssl-sys`.
  - All wiring must reference the concrete connector APIs captured in §1.6; do **not** reimplement PEM parsing unless unavoidable.

### 3.3 Config agent networking
- **File**: `crates/pavis/src/agent/worker/agent.rs`
  - Switching `reqwest` to native-tls is optional and may be deferred; primary objective is Pingora runtime TLS migration. If/when native-tls is adopted, document Linux/macOS/Windows assumptions for `make ci-local` and add CA override guidance. For this migration, keeping rustls here is acceptable to reduce blast radius.

### 3.4 Remove rustls references
- Clean up all `use rustls::*` imports and helper functions (e.g., `create_root_store`). Replace functionality with OpenSSL equivalents or Pingora-provided helpers.

### 3.5 Tests & fixtures
- Unskip TLS security suites (`tests/suites/pavis/70`, `71`, `74`, `75`, `76`). Verify they pass once OpenSSL backend handles per-peer CA bundles and mTLS.
- Each script already generates certs using `openssl` CLI; no fixture repo files needed.

| Suite | Expected validation | Runtime wiring dependency |
| --- | --- | --- |
| `70_security_tls.sh` | Basic inbound TLS termination (cert/key load, HTTPS responses). | §3.1 server TLS wiring must successfully load listener cert/key without client-auth. |
| `71_security_inbound_mtls.sh` | Enforces inbound mTLS (client cert verification). | §3.1 client-auth path must configure Pingora OpenSSL verify/CA hooks. |
| `74_security_mtls_outbound.sh` | Outbound mTLS with custom upstream CA and client cert. | §3.2 upstream connector must honor per-upstream CA bundles and client cert/key. |
| `76_security_mtls_chain_mode.sh` | Validates outbound client cert chain handling modes. | §3.2 must correctly propagate chain files (or equivalents) to the OpenSSL connector. |

### 3.6 Documentation & Product Scope
- Update `README.md` / `docs/BUILDING.md` with OpenSSL requirement, note canonical `make binary-build` command, and highlight that backend parity now requires OpenSSL (ties into ROADMAP Phase 7.5).
- Declare explicitly: **OpenSSL is the only supported TLS backend** after this migration. rustls (and other stacks) are neither supported nor tested in CI. This must be enforced via non-optional Cargo features, so `cargo build -p pavis` always links OpenSSL Pingora.
- Require runtime startup logs to emit `TLS backend: OpenSSL (only supported backend)` so operators know which stack is active.

## 4. Implementation Gates
- **Gate A — Build Gate**: `cargo build -p pavis` (or `make binary-build CRATE=pavis`) must succeed with OpenSSL-enabled dependencies before touching TLS wiring. If this gate fails, stop and fix dependency/CI setup first.
- **Gate B — TLS Bring-up Gate**: `tests/suites/pavis/70_security_tls.sh` must pass (proving baseline HTTPS) before unlocking inbound mTLS, outbound custom CA, or chain-mode tests. Do not attempt suites 71/74/76 until Gate B succeeds.

## 5. Verification Checklist (post-migration)
1. `make test`
2. `make binary-build CRATE=workspace`
3. `make e2e-pavis-binary` and `make e2e-integrated-binary`
4. Targeted TLS suites (after unskip):
   - `make e2e-pavis CASE=70_security_tls.sh`
   - `make e2e-pavis CASE=71_security_inbound_mtls.sh`
   - `make e2e-pavis CASE=74_security_mtls_outbound.sh`
   - `make e2e-pavis CASE=75_security_tls_sni_auto.sh`
   - `make e2e-pavis CASE=76_security_mtls_chain_mode.sh`
5. Optional: `cargo audit` to record new OpenSSL deps.

This plan scopes changes to the runtime crate, its Cargo metadata, CI tooling, and TLS-specific tests/documentation, enabling a clean migration from rustls to OpenSSL.
