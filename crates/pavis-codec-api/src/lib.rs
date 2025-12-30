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

#[derive(Debug, Clone)]
pub struct CheckedArtifact(pub Artifact);

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
    use pavis_core::runtime::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType,
        Route, ServerConfig, TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
                _ => Ok(CheckedArtifact(art)),
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
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Stdout,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "upstream1".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: 8080,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "upstream1".to_string(),
                        weight: 1,
                    }],
                }],
            }],
        }
    }

    fn invalid_config() -> pavis_core::RuntimeConfig {
        let mut cfg = valid_config();
        cfg.upstreams[0].name = String::new();
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
    fn materialize_returns_validated_config() {
        let codec = MockCodec::new(Mode::Ok);
        let cfg = codec
            .materialize(test_artifact(), CompactionLevel::Off)
            .expect("materialize");
        assert_eq!(cfg.upstreams.len(), 1);
        assert_eq!(cfg.upstreams[0].name, "upstream1");
    }

    #[test]
    fn codec_error_implements_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<CodecError>();
    }
}
