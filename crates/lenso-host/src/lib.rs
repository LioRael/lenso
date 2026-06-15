//! Narrow host boot helpers for Lenso applications.
//!
//! This crate is an internal bridge toward the future `lenso` host feature. It
//! keeps starter hosts away from deep `app-*` and `platform-*` imports while the
//! final public facade shape is still being validated.

/// Start the Lenso API host from environment configuration.
pub async fn run_api_from_env() -> anyhow::Result<()> {
    app_api::run_from_env().await
}

/// Start the Lenso worker from environment configuration.
pub async fn run_worker_from_env() -> anyhow::Result<()> {
    app_worker::run_from_env().await
}

/// Apply Lenso migrations from environment configuration.
pub async fn run_migrations_from_env() -> anyhow::Result<()> {
    app_migrate::run_from_env().await
}
