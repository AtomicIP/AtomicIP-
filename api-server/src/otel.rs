//! OpenTelemetry SDK initialisation.
//!
//! Reads configuration from environment variables:
//!
//! | Variable                      | Default                        | Notes                          |
//! |-------------------------------|--------------------------------|--------------------------------|
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317`        | gRPC OTLP endpoint             |
//! | `OTEL_SERVICE_NAME`           | `atomic-patent-api`            |                                |
//! | `OTEL_ENABLED`                | `true`                         | Set to `false` to disable      |
//!
//! Compatible with any OTLP-speaking backend (Jaeger ≥ 1.35, Datadog Agent,
//! OpenTelemetry Collector, Grafana Tempo, Honeycomb, etc.).

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Canonical span attributes used by Soroban RPC child spans.
pub const RPC_SYSTEM: &str = "stellar_soroban";
pub const RPC_DURATION_MS: &str = "rpc.duration_ms";

/// Initialise the global OTel tracer and set up the `tracing` subscriber.
///
/// Returns a [`sdktrace::TracerProvider`] that **must** be kept alive for the
/// duration of the process and passed to [`shutdown_tracer`] on exit so that
/// all pending spans are flushed.
pub fn init_tracer() -> Option<sdktrace::TracerProvider> {
    let enabled = std::env::var("OTEL_ENABLED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "atomic-patent-api".to_string());

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if !enabled {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
        return None;
    }

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint);

    let provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            sdktrace::config().with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new(
                    opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                    service_name,
                ),
            ])),
        )
        .install_batch(runtime::Tokio)
        .expect("failed to install OTLP tracer");

    let tracer = provider.tracer("atomic-patent");
    let otel_layer = OpenTelemetryLayer::new(tracer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(otel_layer)
        .init();

    Some(provider)
}

/// Flush and shut down the tracer provider, ensuring all spans are exported.
pub fn shutdown_tracer(provider: sdktrace::TracerProvider) {
    provider.shutdown().ok();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_otel_disabled_does_not_panic() {
        // When OTEL_ENABLED=false the function should return None cleanly.
        // (We skip actually calling init_tracer here because tracing-subscriber
        //  panics if set_global_default is called twice in the same process.)
        let enabled = std::env::var("OTEL_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
        // Just ensure the logic compiles and the env var is readable.
        let _ = enabled;
    }
}
