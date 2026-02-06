use crate::telemetry::metrics::MetricsRegistry;
use async_trait::async_trait;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{SpanExporter as OtlpSpanExporter, WithExportConfig};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{
    Sampler, SdkTracerProvider, SpanData, SpanExporter as SdkSpanExporter,
};
use pingora::services::Service;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::Registry,
};

/// Handle to the active tracing runtime (provider).
/// Stored in OnceLock and accessed by Proxy.
#[derive(Debug)]
pub struct TracingRuntime {
    provider: SdkTracerProvider,
}

impl TracingRuntime {
    pub fn shutdown(&self) {
        if let Err(error) = self.provider.shutdown() {
            ::tracing::warn!(%error, "Failed to shut down tracing provider cleanly");
        }
    }
}

pub trait TracingMetricsRecorder: Send + Sync {
    fn record_span_created(&self);
    fn record_span_exported(&self);
    fn record_tracing_export_error(&self);
}

type DynTracingMetrics = Arc<dyn TracingMetricsRecorder>;

impl TracingMetricsRecorder for MetricsRegistry {
    fn record_span_created(&self) {
        MetricsRegistry::record_span_created(self);
    }

    fn record_span_exported(&self) {
        MetricsRegistry::record_span_exported(self);
    }

    fn record_tracing_export_error(&self) {
        MetricsRegistry::record_tracing_export_error(self);
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
    metrics: Option<Arc<MetricsRegistry>>,
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
                let dyn_metrics: DynTracingMetrics = metrics.clone();
                let metrics_layer = SpanMetricsLayer::new(dyn_metrics);
                Box::new(metrics_layer.and_then(layer))
            } else {
                Box::new(layer)
            };
            handle.reload(boxed_layer);
        }
        return;
    }

    let exporter_result = OtlpSpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build();

    match exporter_result {
        Ok(exporter) => {
            let exporter = if let Some(metrics) = &metrics {
                let dyn_metrics: DynTracingMetrics = metrics.clone();
                MetricsSpanExporter::new(exporter, Some(dyn_metrics))
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

            let resource = opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name.to_string())
                .build();

            let provider = SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_sampler(sampler)
                .with_resource(resource)
                .build();

            let tracer = provider.tracer("pavis");

            let layer = tracing_opentelemetry::layer().with_tracer(tracer);
            let boxed_layer: TracingLayer = if let Some(metrics) = &metrics {
                let dyn_metrics: DynTracingMetrics = metrics.clone();
                let metrics_layer = SpanMetricsLayer::new(dyn_metrics);
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
    metrics: DynTracingMetrics,
}

impl SpanMetricsLayer {
    fn new(metrics: DynTracingMetrics) -> Self {
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
    metrics: Option<DynTracingMetrics>,
}

impl<E> MetricsSpanExporter<E> {
    fn new(inner: E, metrics: Option<DynTracingMetrics>) -> Self {
        Self { inner, metrics }
    }
}

impl<E> fmt::Debug for MetricsSpanExporter<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricsSpanExporter").finish()
    }
}

impl<E> SdkSpanExporter for MetricsSpanExporter<E>
where
    E: SdkSpanExporter,
{
    fn export(
        &self,
        batch: Vec<SpanData>,
    ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
        let metrics = self.metrics.clone();
        let fut = self.inner.export(batch);
        async move {
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
        }
    }

    fn shutdown_with_timeout(&mut self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn shutdown(&mut self) -> OTelSdkResult {
        self.inner.shutdown()
    }

    fn force_flush(&mut self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &opentelemetry_sdk::Resource) {
        self.inner.set_resource(resource);
    }
}

/// Background service that initializes and manages OpenTelemetry.
pub struct TracingService {
    config: pavis_core::TracingPolicy,
    service_name: String,
    reload_handle: Option<ReloadHandle>,
    runtime_slot: Arc<OnceLock<TracingRuntime>>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl TracingService {
    pub fn new(
        config: pavis_core::TracingPolicy,
        service_name: String,
        reload_handle: Option<ReloadHandle>,
        runtime_slot: Arc<OnceLock<TracingRuntime>>,
        metrics: Option<Arc<MetricsRegistry>>,
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
            let provider = runtime.provider.clone();
            let flush = tokio::task::spawn_blocking(move || {
                if let Err(error) = provider.force_flush() {
                    ::tracing::warn!(%error, "Failed to flush tracing provider");
                }
                if let Err(error) = provider.shutdown() {
                    ::tracing::warn!(%error, "Failed to shut down tracing provider cleanly");
                }
            });
            if let Err(error) = timeout(Duration::from_secs(5), flush).await {
                ::tracing::warn!(%error, "Tracing shutdown timed out");
            }
        }
    }

    fn name(&self) -> &str {
        "tracing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_exporter_prometheus::PrometheusBuilder;
    use opentelemetry_sdk::error::OTelSdkError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tracing::Level;
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Default)]
    struct TestTracingMetrics {
        spans_created: AtomicUsize,
        exports: AtomicUsize,
        errors: AtomicUsize,
    }

    impl TestTracingMetrics {
        fn spans(&self) -> usize {
            self.spans_created.load(Ordering::SeqCst)
        }

        fn exports(&self) -> usize {
            self.exports.load(Ordering::SeqCst)
        }

        fn errors(&self) -> usize {
            self.errors.load(Ordering::SeqCst)
        }
    }

    impl TracingMetricsRecorder for TestTracingMetrics {
        fn record_span_created(&self) {
            self.spans_created.fetch_add(1, Ordering::SeqCst);
        }

        fn record_span_exported(&self) {
            self.exports.fetch_add(1, Ordering::SeqCst);
        }

        fn record_tracing_export_error(&self) {
            self.errors.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum ExportOutcome {
        Success,
        Failure,
    }

    #[derive(Debug)]
    struct TestExporter {
        outcome: ExportOutcome,
    }

    impl TestExporter {
        fn success() -> Self {
            Self {
                outcome: ExportOutcome::Success,
            }
        }

        fn failure() -> Self {
            Self {
                outcome: ExportOutcome::Failure,
            }
        }
    }

    impl SdkSpanExporter for TestExporter {
        fn export(
            &self,
            _batch: Vec<SpanData>,
        ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
            let outcome = self.outcome;
            async move {
                match outcome {
                    ExportOutcome::Success => Ok(()),
                    ExportOutcome::Failure => {
                        Err(OTelSdkError::InternalFailure("export failed".into()))
                    }
                }
            }
        }

        fn shutdown_with_timeout(&mut self, _timeout: Duration) -> OTelSdkResult {
            Ok(())
        }

        fn shutdown(&mut self) -> OTelSdkResult {
            Ok(())
        }

        fn force_flush(&mut self) -> OTelSdkResult {
            Ok(())
        }

        fn set_resource(&mut self, _resource: &opentelemetry_sdk::Resource) {}
    }

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

    #[test]
    fn span_metrics_layer_records_http_spans() {
        let metrics = Arc::new(TestTracingMetrics::default());
        let dyn_metrics: DynTracingMetrics = metrics.clone();
        let layer = SpanMetricsLayer::new(dyn_metrics);
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "http_request");
            span.in_scope(|| {});
        });

        assert_eq!(metrics.spans(), 1);
    }

    #[tokio::test]
    async fn metrics_span_exporter_reports_success_and_error() {
        let metrics_ok = Arc::new(TestTracingMetrics::default());
        let exporter_ok = MetricsSpanExporter::new(
            TestExporter::success(),
            Some(metrics_ok.clone() as DynTracingMetrics),
        );
        exporter_ok
            .export(Vec::new())
            .await
            .expect("export should succeed");
        assert_eq!(metrics_ok.exports(), 1);

        let metrics_err = Arc::new(TestTracingMetrics::default());
        let exporter_err = MetricsSpanExporter::new(
            TestExporter::failure(),
            Some(metrics_err.clone() as DynTracingMetrics),
        );
        exporter_err
            .export(Vec::new())
            .await
            .expect_err("export should fail");
        assert_eq!(metrics_err.errors(), 1);
    }

    #[tokio::test]
    async fn metrics_span_exporter_management_calls_delegate() {
        let mut exporter = MetricsSpanExporter::new(TestExporter::success(), None);
        exporter
            .shutdown_with_timeout(Duration::from_millis(1))
            .expect("shutdown_with_timeout delegates");
        exporter.force_flush().expect("force_flush delegates");
        exporter.shutdown().expect("shutdown delegates");
        exporter.set_resource(&opentelemetry_sdk::Resource::builder().build());
    }

    #[tokio::test]
    async fn maybe_init_tracing_jaeger_provider() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Jaeger,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:14268".to_string(),
        };
        let reload = ReloadHandle::new();
        let slot = Arc::new(OnceLock::new());

        maybe_init_tracing(&policy, "svc", Some(&reload), &slot, None);

        assert!(slot.get().is_some());
        let guard = reload.inner.read().unwrap();
        assert!(guard.is_some());
    }

    #[tokio::test]
    async fn maybe_init_tracing_zipkin_provider() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Zipkin,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:9411".to_string(),
        };
        let reload = ReloadHandle::new();
        let slot = Arc::new(OnceLock::new());

        maybe_init_tracing(&policy, "svc", Some(&reload), &slot, None);

        assert!(slot.get().is_some());
        let guard = reload.inner.read().unwrap();
        assert!(guard.is_some());
    }

    #[tokio::test]
    async fn maybe_init_tracing_initializes_runtime_and_reload() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Otlp,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:4317".to_string(),
        };
        let reload = ReloadHandle::new();
        let slot = Arc::new(OnceLock::new());

        maybe_init_tracing(&policy, "svc", Some(&reload), &slot, None);

        assert!(slot.get().is_some(), "runtime should be installed");
        let guard = reload.inner.read().unwrap();
        assert!(guard.is_some(), "layer should be installed");
    }

    #[test]
    fn maybe_init_tracing_skips_when_disabled() {
        let policy = pavis_core::TracingPolicy::Disabled;
        let reload = ReloadHandle::new();
        let slot = Arc::new(OnceLock::new());

        maybe_init_tracing(&policy, "svc", Some(&reload), &slot, None);

        assert!(slot.get().is_none());
        let guard = reload.inner.read().unwrap();
        assert!(guard.is_none());
    }

    #[tokio::test]
    async fn tracing_service_initializes_and_shuts_down() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Otlp,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:4317".to_string(),
        };
        let reload = ReloadHandle::new();
        let slot = Arc::new(OnceLock::new());
        let metrics = None;

        let mut service = TracingService::new(
            policy,
            "svc".to_string(),
            Some(reload.clone()),
            slot.clone(),
            metrics,
        );

        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move {
            service.start_service(None, rx, 1).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        tx.send(true).expect("should signal shutdown");
        handle.await.expect("service should finish");

        assert!(slot.get().is_some());
        let guard = reload.inner.read().unwrap();
        assert!(guard.is_some(), "layer should be installed by service");
    }

    struct SpyLayer {
        on_new_span: Arc<AtomicBool>,
        on_event: Arc<AtomicBool>,
        on_enter: Arc<AtomicBool>,
        on_exit: Arc<AtomicBool>,
    }

    impl SpyLayer {
        fn new() -> Self {
            Self {
                on_new_span: Arc::new(AtomicBool::new(false)),
                on_event: Arc::new(AtomicBool::new(false)),
                on_enter: Arc::new(AtomicBool::new(false)),
                on_exit: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl<S: Subscriber> Layer<S> for SpyLayer {
        fn on_new_span(
            &self,
            _attrs: &tracing::span::Attributes<'_>,
            _id: &Id,
            _ctx: Context<'_, S>,
        ) {
            self.on_new_span.store(true, Ordering::SeqCst);
        }
        fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
            self.on_event.store(true, Ordering::SeqCst);
        }
        fn on_enter(&self, _id: &Id, _ctx: Context<'_, S>) {
            self.on_enter.store(true, Ordering::SeqCst);
        }
        fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
            self.on_exit.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn reloadable_layer_delegation_full() {
        use tracing_subscriber::prelude::*;

        #[derive(Default)]
        struct FullSpy {
            new_span: Arc<AtomicBool>,
            event: Arc<AtomicBool>,
            enter: Arc<AtomicBool>,
            exit: Arc<AtomicBool>,
            record: Arc<AtomicBool>,
            close: Arc<AtomicBool>,
        }
        impl<S: Subscriber> Layer<S> for FullSpy {
            fn on_new_span(
                &self,
                _attrs: &tracing::span::Attributes<'_>,
                _id: &Id,
                _ctx: Context<'_, S>,
            ) {
                self.new_span.store(true, Ordering::SeqCst);
            }
            fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
                self.event.store(true, Ordering::SeqCst);
            }
            fn on_enter(&self, _id: &Id, _ctx: Context<'_, S>) {
                self.enter.store(true, Ordering::SeqCst);
            }
            fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
                self.exit.store(true, Ordering::SeqCst);
            }
            fn on_record(
                &self,
                _span: &Id,
                _values: &tracing::span::Record<'_>,
                _ctx: Context<'_, S>,
            ) {
                self.record.store(true, Ordering::SeqCst);
            }
            fn on_close(&self, _id: Id, _ctx: Context<'_, S>) {
                self.close.store(true, Ordering::SeqCst);
            }
        }

        let spy = Arc::new(FullSpy::default());
        let reload = ReloadHandle::new();
        reload.reload(Box::new(FullSpy {
            new_span: spy.new_span.clone(),
            event: spy.event.clone(),
            enter: spy.enter.clone(),
            exit: spy.exit.clone(),
            record: spy.record.clone(),
            close: spy.close.clone(),
        }));

        let subscriber = Registry::default().with(reload);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "test", f = 1);
            span.record("f", 2);
            let _guard = span.enter();
            tracing::info!("event");
        });

        assert!(spy.new_span.load(Ordering::SeqCst));
        assert!(spy.event.load(Ordering::SeqCst));
        assert!(spy.enter.load(Ordering::SeqCst));
        assert!(spy.exit.load(Ordering::SeqCst));
        assert!(spy.record.load(Ordering::SeqCst));
        assert!(spy.close.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_tracing_service_shutdown_logic() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Otlp,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:4317".to_string(),
        };
        let slot = Arc::new(OnceLock::new());
        let provider = SdkTracerProvider::builder().build();
        slot.set(TracingRuntime { provider }).unwrap();

        let mut service = TracingService::new(policy, "svc".to_string(), None, slot.clone(), None);

        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move {
            service.start_service(None, rx, 1).await;
        });

        tx.send(true).unwrap();
        timeout(Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn reloadable_layer_clone() {
        let layer1 = ReloadHandle::new();
        let layer2 = layer1.clone();

        // Both should share the same inner Arc
        assert!(Arc::ptr_eq(&layer1.inner, &layer2.inner));
    }

    #[test]
    fn reloadable_layer_default() {
        let layer = ReloadHandle::default();
        let guard = layer.inner.read().unwrap();
        assert!(guard.is_none(), "default layer should be empty");
    }

    #[test]
    fn reloadable_layer_reload_updates_inner() {
        let reload = ReloadHandle::new();

        // Initially empty
        {
            let guard = reload.inner.read().unwrap();
            assert!(guard.is_none());
        }

        // Reload with a spy layer
        let spy = SpyLayer::new();
        reload.reload(Box::new(spy));

        // Now should have a layer
        {
            let guard = reload.inner.read().unwrap();
            assert!(guard.is_some(), "reload should install layer");
        }
    }

    #[test]
    fn tracing_runtime_shutdown_succeeds() {
        // Create a minimal tracer provider
        let provider = SdkTracerProvider::builder().build();
        let runtime = TracingRuntime { provider };

        // Shutdown should not panic
        runtime.shutdown();
    }

    #[test]
    fn test_tracing_service_name() {
        let slot = Arc::new(OnceLock::new());
        let service = TracingService::new(
            pavis_core::TracingPolicy::Disabled,
            "svc".to_string(),
            None,
            slot,
            None,
        );
        assert_eq!(service.name(), "tracing");
    }

    #[test]
    fn maybe_init_tracing_existing_runtime_no_handle() {
        let policy = pavis_core::TracingPolicy::Enabled {
            provider: pavis_core::TracingProvider::Otlp,
            sampling: pavis_core::SampleRate(100),
            endpoint: "http://127.0.0.1:4317".to_string(),
        };
        let slot = Arc::new(OnceLock::new());
        let provider = SdkTracerProvider::builder().build();
        slot.set(TracingRuntime { provider }).unwrap();

        // Should return early and do nothing
        maybe_init_tracing(&policy, "svc", None, &slot, None);
    }

    #[test]
    fn test_tracing_metrics_recorder() {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let registry = MetricsRegistry {
            _handle: Arc::new(handle),
            labels: Arc::new(crate::telemetry::metrics::test_exports::new_labels()),
        };

        // These should not panic
        registry.record_span_created();
        registry.record_span_exported();
        registry.record_tracing_export_error();
    }
}
