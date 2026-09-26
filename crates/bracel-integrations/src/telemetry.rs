use crate::Error;
use opentelemetry::{KeyValue, trace::TracerProvider};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    Resource,
    trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider},
};
use std::time::Duration;

pub struct Telemetry {
    provider: SdkTracerProvider,
}
impl Telemetry {
    /// Endpoint includes /v1/traces. SDK-owned background threads isolate exporter outages.
    pub fn otlp(service: &str, endpoint: &str, sample_ratio: f64) -> Result<Self, Error> {
        if service.is_empty()
            || service.len() > 100
            || !(0.0..=1.0).contains(&sample_ratio)
            || !(endpoint.starts_with("https://") || endpoint.starts_with("http://127.0.0.1:"))
        {
            return Err(Error::Configuration);
        }
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .with_timeout(Duration::from_secs(3))
            .with_max_request_body_size(1024 * 1024)
            .with_retry_policy(opentelemetry_otlp::RetryPolicy::default().with_max_retries(0))
            .build()
            .map_err(|_| Error::Configuration)?;
        let processor = BatchSpanProcessor::builder(exporter)
            .with_batch_config(
                BatchConfigBuilder::default()
                    .with_max_queue_size(2048)
                    .with_max_export_batch_size(256)
                    .build(),
            )
            .build();
        let provider = SdkTracerProvider::builder()
            .with_resource(
                Resource::builder_empty()
                    .with_attribute(KeyValue::new("service.name", service.to_owned()))
                    .build(),
            )
            .with_sampler(Sampler::TraceIdRatioBased(sample_ratio))
            .with_span_processor(processor)
            .build();
        Ok(Self { provider })
    }
    pub fn tracer(&self) -> opentelemetry_sdk::trace::Tracer {
        self.provider.tracer("bracel")
    }
    pub fn install_tracing(&self) -> Result<(), Error> {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json().with_target(false))
            .with(tracing_opentelemetry::layer().with_tracer(self.tracer()))
            .try_init()
            .map_err(|_| Error::Configuration)
    }
    pub fn shutdown(self) -> Result<(), Error> {
        self.provider
            .shutdown_with_timeout(Duration::from_secs(4))
            .map_err(|_| Error::Unavailable)
    }
}
pub use opentelemetry;

/// Mount inside Bracel's HTTP lifecycle. Trace context is correlation, never identity.
pub async fn propagate(
    request: bracel::axum::extract::Request,
    next: bracel::axum::middleware::Next,
) -> bracel::axum::response::Response {
    use opentelemetry::propagation::TextMapPropagator;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    struct Headers<'a>(&'a bracel::axum::http::HeaderMap);
    impl opentelemetry::propagation::Extractor for Headers<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|v| v.to_str().ok())
        }
        fn keys(&self) -> Vec<&str> {
            vec!["traceparent", "tracestate"]
        }
    }
    let parent = opentelemetry_sdk::propagation::TraceContextPropagator::new()
        .extract(&Headers(request.headers()));
    use tracing::Instrument;
    let span = tracing::info_span!("http_inbound");
    let _ = span.set_parent(parent);
    next.run(request).instrument(span).await
}

pub fn current_headers() -> Vec<(String, String)> {
    use opentelemetry::propagation::TextMapPropagator;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    struct Headers(Vec<(String, String)>);
    impl opentelemetry::propagation::Injector for Headers {
        fn set(&mut self, key: &str, value: String) {
            self.0.push((key.into(), value));
        }
    }
    let mut headers = Headers(Vec::new());
    opentelemetry_sdk::propagation::TraceContextPropagator::new()
        .inject_context(&tracing::Span::current().context(), &mut headers);
    headers.0
}
