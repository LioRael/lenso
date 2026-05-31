use crate::config::TelemetryConfig;
use crate::error::AppResult;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init(config: &TelemetryConfig) -> AppResult<()> {
    let env_filter = EnvFilter::try_new(&config.log_level)
        .or_else(|_| EnvFilter::try_new("info"))
        .expect("fallback tracing filter must be valid");

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json())
        .try_init()
        .ok();

    Ok(())
}
