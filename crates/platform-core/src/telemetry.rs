use crate::config::{LogFormat, TelemetryConfig};
use crate::error::{AppError, AppResult, ErrorCode};
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

static OTEL_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

pub fn init(config: &TelemetryConfig) -> AppResult<()> {
    let env_filter = EnvFilter::try_new(&config.log_level)
        .or_else(|_| EnvFilter::try_new("info"))
        .expect("fallback tracing filter must be valid");
    let otel_layer = config.otlp_endpoint.as_deref().and_then(build_otel_layer);

    match (config.log_format, otel_layer) {
        (LogFormat::Compact, Some(otel_layer)) => {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(env_filter)
                .with(fmt::layer().compact().with_target(false))
                .try_init()
                .ok();
        }
        (LogFormat::Compact, None) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().compact().with_target(false))
                .try_init()
                .ok();
        }
        (LogFormat::Json, Some(otel_layer)) => {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(env_filter)
                .with(fmt::layer().json())
                .try_init()
                .ok();
        }
        (LogFormat::Json, None) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .try_init()
                .ok();
        }
    }

    Ok(())
}

pub fn force_flush() -> AppResult<()> {
    let Some(provider) = OTEL_PROVIDER.get() else {
        return Ok(());
    };

    provider.force_flush().map_err(|error| {
        AppError::new(ErrorCode::ExternalDependency, "OpenTelemetry flush failed")
            .with_source(error)
    })
}

fn build_otel_layer(
    endpoint: &str,
) -> Option<
    tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >,
> {
    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.to_owned())
        .build()
    {
        Ok(exporter) => exporter,
        Err(error) => {
            eprintln!("failed to configure OTLP span exporter: {error}");
            return None;
        }
    };

    let provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(
            Resource::builder()
                .with_service_name("lenso")
                .with_attribute(KeyValue::new(
                    "deployment.environment.name",
                    std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_owned()),
                ))
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer("lenso-runtime");
    opentelemetry::global::set_tracer_provider(provider.clone());
    OTEL_PROVIDER.set(provider).ok();

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}
