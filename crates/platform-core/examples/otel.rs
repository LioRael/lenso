use platform_core::{
    LogFormat, RuntimeSpanAttributes, TelemetryConfig, record_runtime_span_attributes, telemetry,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_owned());
    let correlation_id = example_correlation_id();

    telemetry::init(&TelemetryConfig {
        log_level: "info".to_owned(),
        log_format: LogFormat::Compact,
        otlp_endpoint: Some(endpoint),
    })?;

    emit_outbox_span(&correlation_id);
    emit_function_span(&correlation_id);
    telemetry::force_flush()?;

    println!("OTEL_EXAMPLE_CORRELATION_ID={correlation_id}");
    Ok(())
}

fn emit_outbox_span(correlation_id: &str) {
    let span = tracing::info_span!(
        "outbox_publish",
        lenso.correlation_id = tracing::field::Empty,
        lenso.story_id = tracing::field::Empty,
        lenso.outbox_event_id = tracing::field::Empty,
        lenso.execution.kind = tracing::field::Empty,
        lenso.execution.name = tracing::field::Empty,
    );
    record_runtime_span_attributes(
        &span,
        &RuntimeSpanAttributes::outbox(
            correlation_id.to_owned(),
            format!("{correlation_id}:outbox"),
            "otel_example.runtime_event",
        ),
    );
}

fn emit_function_span(correlation_id: &str) {
    let span = tracing::info_span!(
        "function_run",
        lenso.correlation_id = tracing::field::Empty,
        lenso.story_id = tracing::field::Empty,
        lenso.function_run_id = tracing::field::Empty,
        lenso.execution.kind = tracing::field::Empty,
        lenso.execution.name = tracing::field::Empty,
    );
    record_runtime_span_attributes(
        &span,
        &RuntimeSpanAttributes::function(
            correlation_id.to_owned(),
            format!("{correlation_id}:function"),
            "otel_example.runtime_function",
        ),
    );
}

fn example_correlation_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("otel_example_{timestamp}")
}
