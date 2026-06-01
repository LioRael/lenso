use crate::config::{LogFormat, TelemetryConfig};
use crate::error::AppResult;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init(config: &TelemetryConfig) -> AppResult<()> {
    let env_filter = EnvFilter::try_new(&config.log_level)
        .or_else(|_| EnvFilter::try_new("info"))
        .expect("fallback tracing filter must be valid");

    match config.log_format {
        LogFormat::Compact => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().compact().with_target(false))
                .try_init()
                .ok();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .try_init()
                .ok();
        }
    }

    Ok(())
}
