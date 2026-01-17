use crate::router::Router;
use crate::upstream::Manager;
use arc_swap::ArcSwap;
use pavis_core::{
    AccessLogPolicy, ListenerBuilder, ListenerName, Metrics, RuntimeConfigBuilder, ServiceName,
    Telemetry, ValidatedRuntimeConfig, WorkerCount,
};
use std::sync::Arc;

pub struct RuntimeState {
    pub config: ValidatedRuntimeConfig,
    pub router: Arc<Router>,
    pub upstream_manager: Manager,
}

impl RuntimeState {
    pub fn from_config(config: &ValidatedRuntimeConfig) -> anyhow::Result<Self> {
        let router = Arc::new(Router::new(config.routes.clone())?);
        let upstream_manager = Manager::new(&config.upstreams)?;
        Ok(Self {
            config: config.clone(),
            router,
            upstream_manager,
        })
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address("127.0.0.1:0".parse().expect("default addr"))
            .workers(WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        let empty_config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Error,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(listener)
            .build()
            .expect("config");
        // SAFETY: Default RuntimeConfig is empty and valid.
        let config = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(empty_config) };
        Self {
            config,
            router: Arc::new(Router::new(vec![]).expect("empty router")),
            upstream_manager: Manager::new(&[]).expect("empty upstream manager"),
        }
    }
}

pub struct RuntimeStateHandle {
    inner: ArcSwap<RuntimeState>,
}

impl RuntimeStateHandle {
    pub fn new(state: RuntimeState) -> Self {
        Self {
            inner: ArcSwap::from_pointee(state),
        }
    }

    pub fn load(&self) -> Arc<RuntimeState> {
        self.inner.load_full()
    }

    pub fn store(&self, state: RuntimeState) {
        self.inner.store(Arc::new(state));
    }
}
