use crate::telemetry::metrics::MetricsHandle;
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};
use opentelemetry_sdk::trace::{Config, Sampler};
use pingora::services::Service;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
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

static TRACING_LAYER_POISONED: AtomicBool = AtomicBool::new(false);

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
        let mut inner = match self.inner.write() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        *inner = Some(layer);
    }
}

impl<S> Layer<S> for ReloadableLayer<S>
where
    S: Subscriber,
{
    fn on_layer(&mut self, subscriber: &mut S) {
        let mut guard = match self.inner.write() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_mut() {
            inner.on_layer(subscriber);
        }
    }

    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_new_span(attrs, id, ctx);
        }
    }

    fn on_record(&self, span: &Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_record(span, values, ctx);
        }
    }

    fn on_follows_from(&self, span: &Id, follows: &Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_follows_from(span, follows, ctx);
        }
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: Context<'_, S>) -> bool {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.enabled(metadata, ctx)
        } else {
            true
        }
    }

    fn event_enabled(&self, event: &Event<'_>, ctx: Context<'_, S>) -> bool {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.event_enabled(event, ctx)
        } else {
            true
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_event(event, ctx);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_enter(id, ctx);
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_exit(id, ctx);
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_close(id, ctx);
        }
    }

    fn on_id_change(&self, old: &Id, new: &Id, ctx: Context<'_, S>) {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            inner.on_id_change(old, new, ctx);
        }
    }

    unsafe fn downcast_raw(&self, id: std::any::TypeId) -> Option<*const ()> {
        let guard = match self.inner.read() {
            Ok(inner) => inner,
            Err(poisoned) => {
                if !TRACING_LAYER_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("tracing reload lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(inner) = guard.as_ref() {
            // SAFETY: delegating to inner layer preserves the required TypeId checks.
            unsafe { inner.downcast_raw(id) }
        } else {
            None
        }
    }
}

pub type TracingLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
pub type ReloadHandle = ReloadableLayer<Registry>;

pub fn maybe_init_tracing(
    policy: &pavis_core::TracingPolicy,
    service_name: &str,
    reload_handle: Option<&ReloadHandle>,
    runtime_slot: &Arc<OnceLock<TracingRuntime>>,
    metrics: Option<Arc<MetricsHandle>>,
) {
    let pavis_core::TracingPolicy::Enabled {
        sampling,
        endpoint,
        provider: _,
    } = policy
    else {
        return;
    };

    if let Some(runtime) = runtime_slot.get() {
        if let Some(handle) = reload_handle {
            let tracer = runtime.provider.tracer("pavis");
            let layer = tracing_opentelemetry::layer().with_tracer(tracer);
            let boxed_layer: TracingLayer = if let Some(metrics) = &metrics {
                let metrics_layer = SpanMetricsLayer::new(metrics.clone());
                Box::new(metrics_layer.and_then(layer))
            } else {
                Box::new(layer)
            };
            handle.reload(boxed_layer);
        }
        return;
    }

    let result = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint)
        .build_span_exporter();

    match result {
        Ok(exporter) => {
            let exporter = if let Some(metrics) = &metrics {
                MetricsSpanExporter::new(exporter, Some(metrics.clone()))
            } else {
                MetricsSpanExporter::new(exporter, None)
            };
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
                    service_name.to_string(),
                )]),
            );

            let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .with_config(config)
                .build();

            let tracer = provider.tracer("pavis");

            let layer = tracing_opentelemetry::layer().with_tracer(tracer);
            let boxed_layer: TracingLayer = if let Some(metrics) = &metrics {
                let metrics_layer = SpanMetricsLayer::new(metrics.clone());
                Box::new(metrics_layer.and_then(layer))
            } else {
                Box::new(layer)
            };

            if let Some(handle) = reload_handle {
                handle.reload(boxed_layer);
                ::tracing::info!("Tracing layer initialized and installed");
            } else {
                ::tracing::warn!("No reload handle provided for tracing");
            }

            let runtime = TracingRuntime { provider };
            if runtime_slot.set(runtime).is_err() {
                ::tracing::error!("Tracing runtime already initialized (unexpected)");
            }
        }
        Err(e) => {
            ::tracing::error!(error = %e, "Failed to build OTLP exporter");
        }
    }
}

#[derive(Clone)]
struct SpanMetricsLayer {
    metrics: Arc<MetricsHandle>,
}

impl SpanMetricsLayer {
    fn new(metrics: Arc<MetricsHandle>) -> Self {
        Self { metrics }
    }
}

impl<S> Layer<S> for SpanMetricsLayer
where
    S: Subscriber,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let metadata = attrs.metadata();
        if metadata.name() == "http_request" {
            self.metrics.record_span_created();
        }
        let _ = (attrs, id, ctx);
    }
}

struct MetricsSpanExporter<E> {
    inner: E,
    metrics: Option<Arc<MetricsHandle>>,
}

impl<E> MetricsSpanExporter<E> {
    fn new(inner: E, metrics: Option<Arc<MetricsHandle>>) -> Self {
        Self { inner, metrics }
    }
}

impl<E> fmt::Debug for MetricsSpanExporter<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricsSpanExporter").finish()
    }
}

impl<E> SpanExporter for MetricsSpanExporter<E>
where
    E: SpanExporter,
{
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
        let metrics = self.metrics.clone();
        let fut = self.inner.export(batch);
        Box::pin(async move {
            match fut.await {
                Ok(()) => {
                    if let Some(handle) = metrics.as_ref() {
                        handle.record_span_exported();
                    }
                    Ok(())
                }
                Err(err) => {
                    if let Some(handle) = metrics.as_ref() {
                        handle.record_tracing_export_error();
                    }
                    Err(err)
                }
            }
        })
    }

    fn shutdown(&mut self) {
        self.inner.shutdown();
    }

    fn force_flush(&mut self) -> BoxFuture<'static, ExportResult> {
        self.inner.force_flush()
    }
}

/// Background service that initializes and manages OpenTelemetry.
pub struct TracingService {
    config: pavis_core::TracingPolicy,
    service_name: String,
    reload_handle: Option<ReloadHandle>,
    runtime_slot: Arc<OnceLock<TracingRuntime>>,
    metrics: Option<Arc<MetricsHandle>>,
}

impl TracingService {
    pub fn new(
        config: pavis_core::TracingPolicy,
        service_name: String,
        reload_handle: Option<ReloadHandle>,
        runtime_slot: Arc<OnceLock<TracingRuntime>>,
        metrics: Option<Arc<MetricsHandle>>,
    ) -> Self {
        // Set global propagator for context propagation (sync)
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        Self {
            config,
            service_name,
            reload_handle,
            runtime_slot,
            metrics,
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
        if let pavis_core::TracingPolicy::Enabled { endpoint, .. } = &self.config {
            ::tracing::info!(
                service_name = %self.service_name,
                endpoint = %endpoint,
                "Initializing OpenTelemetry tracing (async)"
            );
            maybe_init_tracing(
                &self.config,
                &self.service_name,
                self.reload_handle.as_ref(),
                &self.runtime_slot,
                self.metrics.clone(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reloadable_layer_recovers_from_poisoned_lock() {
        let mut layer = ReloadableLayer::<Registry>::new();
        let inner = layer.inner.clone();

        let _ = std::panic::catch_unwind(move || {
            let _guard = inner.write().unwrap();
            panic!("poison lock");
        });

        layer.reload(Box::new(tracing_subscriber::fmt::Layer::default()));
        let mut subscriber = Registry::default();
        layer.on_layer(&mut subscriber);
    }
}
