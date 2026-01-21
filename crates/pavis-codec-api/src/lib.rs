use pavis_core::{CoreValidationError, RuntimeConfig, ValidatedRuntimeConfig};
use pavis_ingest_api::Artifact;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("codec check failed")]
    Check(#[source] anyhow::Error),
    #[error("codec compile failed")]
    Compile(#[source] anyhow::Error),
    #[error(transparent)]
    Core(CoreValidationError),
}

impl From<CoreValidationError> for CodecError {
    fn from(err: CoreValidationError) -> Self {
        CodecError::Core(err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionLevel {
    #[default]
    Off,
    Trim,
    Prune,
}

pub struct CheckedArtifact {
    pub artifact: Artifact,
    pub state: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

impl std::fmt::Debug for CheckedArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedArtifact")
            .field("artifact", &self.artifact)
            .field(
                "state",
                &if self.state.is_some() {
                    "Some(...)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

impl CheckedArtifact {
    pub fn new(artifact: Artifact) -> Self {
        Self {
            artifact,
            state: None,
        }
    }

    pub fn with_state(artifact: Artifact, state: impl std::any::Any + Send + Sync) -> Self {
        Self {
            artifact,
            state: Some(std::sync::Arc::new(state)),
        }
    }

    pub fn state<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.state
            .as_ref()
            .and_then(|state| state.downcast_ref::<T>())
    }
}

/// # Codec (Artifact → RuntimeConfig)
///
/// `Codec` defines the **forward-only configuration compilation pipeline**
/// from an opaque input `Artifact` (bytes + provenance) into a
/// **validated, runtime-ready `RuntimeConfig`**.
///
/// This trait is the **authoritative boundary** between:
///
/// - **Ingest layer**
///   - I/O, file watching, network streams
///   - Authentication, retries, backoff
///   - Produces opaque `Artifact` values
///
/// - **Runtime / Core layer**
///   - Canonical configuration schema
///   - Execution semantics
///   - Semantic validation
///
/// The codec layer is responsible for **all source-specific interpretation**
/// (YAML, xDS, CRD, etc), including defaulting and normalization.
///
/// ---
///
/// ## Pipeline Overview
///
/// The pipeline is intentionally **linear, ordered, and non-bypassable**:
///
/// ```text
/// Artifact
///   └─ check        (framing & basic integrity)
///       └─ compile  (parse + normalize + semantic defaults)
///           └─ compact (optional, semantics-preserving)
///               └─ validate_runtime (core canonical validation)
/// ```
///
/// ---
///
/// ## Semantic Boundaries (Hard Rules)
///
/// These rules are **mandatory** and enforced by code review:
///
/// 1. **Semantic defaults**
///    - MUST be fully applied inside `compile`.
///    - After `compile` returns, the `RuntimeConfig` MUST be semantically complete
///      for the given source.
///
/// 2. **Core semantic validation**
///    - MUST NOT be performed inside `compile`.
///    - MUST be performed exactly once, inside `materialize`,
///      via `pavis_core::validate_runtime`.
///
/// 3. **Runtime / Relay / Core**
///    - MUST NOT apply semantic defaults.
///    - MUST treat `RuntimeConfig` as authoritative input.
///
/// ---
///
/// ## Forward-Only Design
///
/// This trait is intentionally **forward-only**.
///
/// Any reverse or rendering logic (e.g. `RuntimeConfig → YAML/xDS`)
/// MUST live in a separate, optional trait. Reversibility is not assumed
/// and must not be required.
///
/// ---
///
/// ## Error Model
///
/// - `Error` MUST be convertible from `CoreValidationError`.
/// - Codec-specific failures (parse errors, invalid source semantics)
///   MUST be represented in the codec error type.
///
pub trait Codec {
    /// Codec-specific error type.
    ///
    /// Requirements:
    /// - MUST be `Send + Sync + 'static` (safe for async + cross-task use).
    /// - MUST implement `From<CoreValidationError>` so canonical validation
    ///   errors can be surfaced without translation loss.
    type Error: std::error::Error + Send + Sync + 'static + From<CoreValidationError>;

    /// Stage 1: Artifact framing and integrity checks.
    ///
    /// Responsibilities:
    /// - Validate artifact framing and declared format.
    /// - Perform basic sanity checks (empty payload, unsupported format, etc).
    /// - Optionally attach ephemeral parsing state to the returned value.
    ///
    /// MUST:
    /// - Reject malformed or unsupported artifacts early.
    ///
    /// MUST NOT:
    /// - Perform DTO parsing.
    /// - Apply semantic defaults.
    /// - Construct or partially construct `RuntimeConfig`.
    fn check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error>;

    /// Stage 2: Compile a checked artifact into a `RuntimeConfig`.
    ///
    /// This is the **semantic materialization boundary** of the codec.
    ///
    /// MUST:
    /// - Parse the source representation (YAML/xDS/CRD/etc).
    /// - Normalize source-specific quirks.
    /// - Apply **all source-specific semantic defaults**.
    /// - Produce a semantically complete `RuntimeConfig`.
    ///
    /// MUST NOT:
    /// - Perform core semantic validation.
    ///   Canonical validation happens exactly once, in `materialize()`.
    ///
    /// The returned `RuntimeConfig` is expected to be:
    /// - Semantically complete
    /// - Structurally valid
    /// - Ready for canonical validation
    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error>;

    /// Optional semantics-preserving compaction step.
    ///
    /// This step MAY:
    /// - Deduplicate repeated structures
    /// - Canonicalize ordering
    /// - Prune redundant fields
    ///
    /// This step MUST:
    /// - Preserve semantics exactly
    ///
    /// This step MUST NOT:
    /// - Introduce semantic defaults
    /// - Change behavior or policy
    /// - Mask invalid configurations
    ///
    /// Default implementation is a no-op.
    fn compact(&self, _cfg: &mut RuntimeConfig, _level: CompactionLevel) {}

    /// Run the full codec pipeline and return a validated runtime configuration.
    ///
    /// This method defines the **only legal execution order**:
    ///
    /// ```text
    /// check → compile → (optional) compact → validate_runtime
    /// ```
    ///
    /// Invariants:
    /// - `compile` MUST NOT call `validate_runtime`.
    /// - Core semantic validation MUST happen exactly once, here.
    /// - Callers MUST NOT bypass this method to obtain runtime configs.
    ///
    /// The returned `ValidatedRuntimeConfig` is guaranteed to be:
    /// - Semantically valid
    /// - Safe for runtime consumption
    /// - Fully materialized
    fn materialize(
        &self,
        art: Artifact,
        level: CompactionLevel,
    ) -> Result<ValidatedRuntimeConfig, Self::Error> {
        let checked = self.check(art)?;
        let mut cfg = self.compile(&checked)?;
        if level != CompactionLevel::Off {
            self.compact(&mut cfg, level);
        }
        pavis_core::validate_runtime(cfg).map_err(Self::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use pavis_core::{
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration,
        Endpoint, EndpointAddr, HeaderPredicates, Host, HttpVersion, IdleTimeout, ListenerName,
        LoadBalancer, MethodPredicate, Metrics, Path, PathMatch, Pool, Port, RetryPolicy, Rewrite,
        RewriteHost, RewritePath, RouteAction, RouteMatcher, ServiceName, Telemetry, Timeout,
        TlsConfig, TlsPolicy, UpstreamId, UpstreamName, VirtualHost, Weight, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::{NonZeroU16, NonZeroU32};

    #[derive(Debug, thiserror::Error, PartialEq, Eq)]
    enum TestError {
        #[error("check error")]
        Check,
        #[error("compile error")]
        Compile,
        #[error(transparent)]
        Core(#[from] CoreValidationError),
    }

    #[derive(Clone, Copy, Debug)]
    enum Mode {
        CheckErr,
        CompileErr,
        InvalidConfig,
        Ok,
    }

    struct MockCodec {
        mode: Mode,
    }

    impl MockCodec {
        fn new(mode: Mode) -> Self {
            Self { mode }
        }
    }

    impl Codec for MockCodec {
        type Error = TestError;

        fn check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error> {
            match self.mode {
                Mode::CheckErr => Err(TestError::Check),
                _ => Ok(CheckedArtifact::new(art)),
            }
        }

        fn compile(
            &self,
            _checked: &CheckedArtifact,
        ) -> Result<pavis_core::RuntimeConfig, Self::Error> {
            match self.mode {
                Mode::CompileErr => Err(TestError::Compile),
                Mode::InvalidConfig => Ok(invalid_config()),
                Mode::Ok | Mode::CheckErr => Ok(valid_config()),
            }
        }
    }

    fn valid_config() -> pavis_core::RuntimeConfig {
        let listener = pavis_core::ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let upstream = pavis_core::UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("upstream1".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream");

        pavis_core::RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Stdout,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .add_upstream(upstream)
            .add_route(VirtualHost {
                host: Host("*".to_string()),
                paths: vec![pavis_core::Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("upstream1".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                }],
            })
            .build()
            .expect("config")
    }

    fn invalid_config() -> pavis_core::RuntimeConfig {
        let mut cfg = valid_config();
        cfg.upstreams[0].name = UpstreamName(String::new());
        cfg
    }

    fn test_artifact() -> Artifact {
        Artifact::new(
            Bytes::from_static(b"artifact"),
            pavis_ingest_api::Format::Yaml,
            pavis_ingest_api::SourceInfo::unknown(),
        )
    }

    #[test]
    fn materialize_propagates_check_error() {
        let codec = MockCodec::new(Mode::CheckErr);
        let err = codec
            .materialize(test_artifact(), CompactionLevel::Off)
            .unwrap_err();
        assert_eq!(err, TestError::Check);
    }

    #[test]
    fn materialize_propagates_compile_error() {
        let codec = MockCodec::new(Mode::CompileErr);
        let err = codec
            .materialize(test_artifact(), CompactionLevel::Off)
            .unwrap_err();
        assert_eq!(err, TestError::Compile);
    }

    #[test]
    fn materialize_propagates_core_validation_error() {
        let codec = MockCodec::new(Mode::InvalidConfig);
        let err = codec
            .materialize(test_artifact(), CompactionLevel::Off)
            .unwrap_err();
        assert_eq!(err, TestError::Core(CoreValidationError::EmptyUpstreamName));
    }

    #[test]
    fn materialize_success() {
        let codec = MockCodec::new(Mode::Ok);
        let cfg = codec
            .materialize(test_artifact(), CompactionLevel::Off)
            .expect("ok");
        assert_eq!(cfg.listeners.len(), 1);
        assert_eq!(cfg.upstreams.len(), 1);
        assert_eq!(cfg.routes.len(), 1);
    }

    #[test]
    fn codec_error_from_core_validation_error() {
        let err: CodecError = CoreValidationError::EmptyUpstreamName.into();
        assert!(matches!(
            err,
            CodecError::Core(CoreValidationError::EmptyUpstreamName)
        ));
        assert!(err.to_string().contains("empty upstream name"));
    }

    #[test]
    fn test_codec_error_display() {
        let check_err = CodecError::Check(anyhow::anyhow!("fail"));
        assert!(check_err.to_string().contains("codec check failed"));

        let compile_err = CodecError::Compile(anyhow::anyhow!("fail"));
        assert!(compile_err.to_string().contains("codec compile failed"));
    }

    #[test]
    fn checked_artifact_new() {
        let artifact = test_artifact();
        let checked = CheckedArtifact::new(artifact.clone());
        assert_eq!(checked.artifact.bytes, artifact.bytes);
        assert!(checked.state.is_none());
        assert_eq!(checked.state::<u32>(), None);

        let debug = format!("{:?}", checked);
        eprintln!("DEBUG NEW: {}", debug);
        assert!(debug.contains("state: \"None\""));
    }

    #[test]
    fn checked_artifact_state() {
        let artifact = test_artifact();
        let checked = CheckedArtifact::with_state(artifact, 42u32);
        assert_eq!(checked.state::<u32>(), Some(&42));
        assert_eq!(checked.state::<String>(), None);

        let debug = format!("{:?}", checked);
        eprintln!("DEBUG STATE: {}", debug);
        assert!(debug.contains("state: \"Some(...)\""));
    }

    #[test]
    fn compact_default_impl_is_noop() {
        let codec = MockCodec::new(Mode::Ok);
        let cfg = codec
            .materialize(test_artifact(), CompactionLevel::Trim)
            .expect("ok");
        // MockCodec uses default compact implementation which does nothing
        assert_eq!(cfg.listeners.len(), 1);
    }
}
