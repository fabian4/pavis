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
}

pub trait Codec {
    type Error: std::error::Error + Send + Sync + 'static + From<CoreValidationError>;

    fn check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error>;

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error>;

    fn pack(&self, cfg: &RuntimeConfig) -> Result<Artifact, Self::Error>;

    fn compact(&self, _cfg: &mut RuntimeConfig, _level: CompactionLevel) {}

    fn materialize(
        &self,
        art: Artifact,
        level: CompactionLevel,
    ) -> Result<ValidatedRuntimeConfig, Self::Error> {
        let checked = self.check(art)?;
        let mut cfg = self.compile(&checked)?;
        // Codec is responsible for population of defaults before validation.
        // self.compact(&mut cfg, level); // Compaction happens here if needed.
        self.compact(&mut cfg, level);
        pavis_core::validate_runtime(cfg).map_err(Self::Error::from)
    }

    fn materialize_default(&self, art: Artifact) -> Result<ValidatedRuntimeConfig, Self::Error> {
        self.materialize(art, CompactionLevel::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use pavis_core::{
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration,
        Endpoint, EndpointAddr, Host, HttpVersion, IdleTimeout, Listener, ListenerName,
        LoadBalancer, Metrics, Path, PathMatch, Pool, Port, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, ServiceName, Telemetry, Timeout, TlsConfig, TlsPolicy, Upstream, UpstreamId,
        UpstreamName, VirtualHost, Weight, WorkerCount,
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

        fn pack(&self, _cfg: &pavis_core::RuntimeConfig) -> Result<Artifact, Self::Error> {
            Ok(Artifact::new(
                Bytes::from_static(b"out"),
                pavis_ingest_api::Format::Yaml,
                pavis_ingest_api::SourceInfo::unknown(),
            ))
        }
    }

    fn valid_config() -> pavis_core::RuntimeConfig {
        pavis_core::RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                workers: WorkerCount::Auto,
                tls: TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Stdout,
                tracing: pavis_core::TracingPolicy::Disabled,
            },
            upstreams: vec![Upstream {
                id: UpstreamId(NonZeroU16::new(1).unwrap()),
                name: UpstreamName("upstream1".to_string()),
                discovery: Discovery::Static,
                balancer: LoadBalancer::RoundRobin,
                protocol: HttpVersion::H1,
                pool: Pool {
                    idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
                    connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
                    max: ConnectionLimit::Unlimited,
                },
                tls: TlsPolicy::Disabled,
                endpoints: vec![Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                        port: Port(NonZeroU16::new(8080).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }],
            }],
            routes: vec![VirtualHost {
                host: Host("*".to_string()),
                paths: vec![pavis_core::Route {
                    matcher: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: pavis_core::HeadersPolicy::Disabled,
                    response_headers: pavis_core::HeadersPolicy::Disabled,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("upstream1".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }],
                }],
            }],
        }
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
    }

    #[test]
    fn checked_artifact_with_state() {
        let artifact = test_artifact();
        let checked = CheckedArtifact::with_state(artifact, 42u32);
        assert!(checked.state.is_some());
        let state = checked.state.unwrap().downcast_ref::<u32>().copied();
        assert_eq!(state, Some(42));
    }
}
