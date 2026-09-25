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
    pub fn shutdown(self) -> Result<(), Error> {
        self.provider
            .shutdown_with_timeout(Duration::from_secs(4))
            .map_err(|_| Error::Unavailable)
    }
}
pub use opentelemetry;
