//! Telemetry setup module for tracing and OpenTelemetry
//!
//! This module provides initialization and shutdown functions for the tracing
//! subsystem, supporting:
//! - Level-based filtering via RUST_LOG
//! - Plain text (default) or JSON output via MCPG_LOG_FORMAT
//! - Optional OpenTelemetry export when OTEL_EXPORTER_OTLP_ENDPOINT is set
//!
//! All local output goes to stderr (stdout is reserved for JSON-RPC).

use opentelemetry_otlp::WithExportConfig;
use std::env;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the tracing subsystem
///
/// This sets up:
/// - A stderr-based formatter (text or JSON based on MCPG_LOG_FORMAT)
/// - An env-filter using RUST_LOG (defaults to "mcpg=info")
/// - Optional OpenTelemetry layer when OTEL_EXPORTER_OTLP_ENDPOINT is set
pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{}=info", crate::config::APP_NAME)));

    let format = env::var("MCPG_LOG_FORMAT").unwrap_or_default();

    // Initialize with or without OpenTelemetry
    if let Some(otel_layer) = init_otel_layer() {
        // With OpenTelemetry
        if format == "json" {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(filter)
                .with(fmt::layer().json().with_writer(std::io::stderr))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(filter)
                .with(fmt::layer().with_writer(std::io::stderr))
                .init();
        }
    } else {
        // Without OpenTelemetry
        if format == "json" {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json().with_writer(std::io::stderr))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(std::io::stderr))
                .init();
        }
    }
}

/// Initialize OpenTelemetry layer if OTLP endpoint is configured
///
/// Returns None if OTEL_EXPORTER_OTLP_ENDPOINT is not set or initialization fails.
fn init_otel_layer() -> Option<
    tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >,
> {
    // Only enable if OTLP endpoint is configured
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;

    let service_name =
        env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| crate::config::APP_NAME.to_string());

    // Build the OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
        .ok()?;

    // Build the tracer provider with batch exporter
    let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", service_name),
        ]))
        .build();

    // Get a tracer from the provider
    use opentelemetry::trace::TracerProvider;
    let tracer = tracer_provider.tracer(crate::config::APP_NAME);

    // Set the global tracer provider
    opentelemetry::global::set_tracer_provider(tracer_provider);

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Shutdown the tracing subsystem
///
/// This flushes any pending OTEL spans before exit.
pub fn shutdown() {
    opentelemetry::global::shutdown_tracer_provider();
}
