use crate::router::Router;
use crate::upstream::Manager;
use arc_swap::ArcSwap;
use pavis_core::ValidatedRuntimeConfig;
use std::sync::Arc;

pub struct RuntimeState {
    pub router: Arc<Router>,
    pub upstream_manager: Manager,
}

impl RuntimeState {
    pub fn from_config(config: &ValidatedRuntimeConfig) -> anyhow::Result<Self> {
        let router = Arc::new(Router::new(config.routes.clone())?);
        let upstream_manager = Manager::new(&config.upstreams);
        Ok(Self {
            router,
            upstream_manager,
        })
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
