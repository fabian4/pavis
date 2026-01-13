use async_trait::async_trait;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Config, Sampler};
use pingora::services::Service;
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::Registry,
};

/// Handle to the active tracing runtime (provider).
/// Stored in OnceLock and accessed by Proxy.
pub struct TracingRuntime {
    provider: opentelemetry_sdk::trace::TracerProvider,
}

impl TracingRuntime {
    pub fn shutdown(&self) {
        // Provider shutdowns on drop
    }
}

/// Custom ReloadableLayer that is transparent to downcasting.
/// This allows tracing-opentelemetry to find the inner OpenTelemetryLayer.
pub struct ReloadableLayer<S> {
    inner: Arc<RwLock<Option<Box<dyn Layer<S> + Send + Sync + 'static>>>>,
}

impl<S> Clone for ReloadableLayer<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> Default for ReloadableLayer<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ReloadableLayer<S> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    pub fn reload(&self, layer: Box<dyn Layer<S> + Send + Sync + 'static>) {
        let mut inner = self.inner.write().unwrap();
        *inner = Some(layer);
    }
}

impl<S> Layer<S> for ReloadableLayer<S>
where
    S: Subscriber,
{
    fn on_layer(&mut self, subscriber: &mut S) {
        if let Some(inner) = self.inner.write().unwrap().as_mut() {
            inner.on_layer(subscriber);
        }
    }

    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_new_span(attrs, id, ctx);
        }
    }

    fn on_record(&self, span: &Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_record(span, values, ctx);
        }
    }

    fn on_follows_from(&self, span: &Id, follows: &Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_follows_from(span, follows, ctx);
        }
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: Context<'_, S>) -> bool {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.enabled(metadata, ctx)
        } else {
            true
        }
    }

    fn event_enabled(&self, event: &Event<'_>, ctx: Context<'_, S>) -> bool {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.event_enabled(event, ctx)
        } else {
            true
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_event(event, ctx);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_enter(id, ctx);
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_exit(id, ctx);
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_close(id, ctx);
        }
    }

    fn on_id_change(&self, old: &Id, new: &Id, ctx: Context<'_, S>) {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            inner.on_id_change(old, new, ctx);
        }
    }

    unsafe fn downcast_raw(&self, id: std::any::TypeId) -> Option<*const ()> {
        if let Some(inner) = self.inner.read().unwrap().as_ref() {
            // Safety: delegating to inner layer
            unsafe { inner.downcast_raw(id) }
        } else {
            None
        }
    }
}

pub type TracingLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
pub type ReloadHandle = ReloadableLayer<Registry>;

/// Background service that initializes and manages OpenTelemetry.
pub struct TracingService {
    config: pavis_core::TracingPolicy,
    service_name: String,
    reload_handle: Option<ReloadHandle>,
    runtime_slot: Arc<OnceLock<TracingRuntime>>,
}

impl TracingService {
    pub fn new(
        config: pavis_core::TracingPolicy,
        service_name: String,
        reload_handle: Option<ReloadHandle>,
        runtime_slot: Arc<OnceLock<TracingRuntime>>,
    ) -> Self {
        Self {
            config,
            service_name,
            reload_handle,
            runtime_slot,
        }
    }
}

#[async_trait]
impl Service for TracingService {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        _threads: usize,
    ) {
        // 1. Initialize Tracing (if enabled)
        if let pavis_core::TracingPolicy::Enabled {
            provider: _, // Only OTLP supported for now
            sampling,
            endpoint,
        } = &self.config
        {
            ::tracing::info!(
                service_name = %self.service_name,
                endpoint = %endpoint,
                "Initializing OpenTelemetry tracing (async)"
            );

            // Initialize OTLP Exporter (Requires Tokio Context)
            let result = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .build_span_exporter();

            match result {
                Ok(exporter) => {
                    let sampling_rate = sampling.0 as f64 / 100.0;
                    let sampler = if sampling_rate >= 1.0 {
                        Sampler::AlwaysOn
                    } else if sampling_rate <= 0.0 {
                        Sampler::AlwaysOff
                    } else {
                        Sampler::TraceIdRatioBased(sampling_rate)
                    };

                    let config = Config::default().with_sampler(sampler).with_resource(
                        opentelemetry_sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                            "service.name",
                            self.service_name.clone(),
                        )]),
                    );

                    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                        .with_config(config)
                        .build();

                    // Set global propagator for context propagation
                    opentelemetry::global::set_text_map_propagator(
                        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
                    );

                    let tracer = provider.tracer("pavis");

                    // Create the tracing-opentelemetry layer
                    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
                    let boxed_layer: TracingLayer = Box::new(layer);

                    // Install the layer via reload handle
                    if let Some(handle) = &self.reload_handle {
                        handle.reload(boxed_layer);
                        ::tracing::info!("Tracing layer initialized and installed");
                    } else {
                        ::tracing::warn!("No reload handle provided for tracing");
                    }

                    // Publish runtime
                    let runtime = TracingRuntime { provider };
                    if self.runtime_slot.set(runtime).is_err() {
                        ::tracing::error!("Tracing runtime already initialized (unexpected)");
                    }
                }
                Err(e) => {
                    ::tracing::error!(error = %e, "Failed to build OTLP exporter");
                }
            }
        }

        // 2. Wait for shutdown
        let _ = shutdown.changed().await;

        // 3. Shutdown logic
        if let Some(runtime) = self.runtime_slot.get() {
            ::tracing::info!("Flushing traces...");
            for _ in runtime.provider.force_flush() {}
            runtime.shutdown();
        }
    }

    fn name(&self) -> &str {
        "tracing"
    }
}
