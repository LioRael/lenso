//! Narrow host boot helpers for Lenso applications.
//!
//! This crate is an internal bridge toward the future `lenso` host feature. It
//! keeps starter hosts away from deep `app-*` and `platform-*` imports while the
//! final public facade shape is still being validated.

pub use app_bootstrap::{HostComposition, HostLinkedModule};

#[derive(Debug, Clone, Default)]
pub struct HostBuilder {
    composition: HostComposition,
}

impl HostBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn linked_module(mut self, module: HostLinkedModule) -> Self {
        self.composition.add_linked_module(module);
        self
    }

    #[must_use]
    pub fn build(self) -> HostComposition {
        self.composition
    }

    pub async fn run_api_from_env(self) -> anyhow::Result<()> {
        run_api_from_env_with_composition(self.composition).await
    }

    pub async fn run_worker_from_env(self) -> anyhow::Result<()> {
        run_worker_from_env_with_composition(self.composition).await
    }

    pub async fn run_migrations_from_env(self) -> anyhow::Result<()> {
        run_migrations_from_env_with_composition(self.composition).await
    }
}

/// Start the Lenso API host from environment configuration.
pub async fn run_api_from_env() -> anyhow::Result<()> {
    run_api_from_env_with_composition(HostComposition::default()).await
}

/// Start the Lenso API host with additional host-owned linked modules.
pub async fn run_api_from_env_with_composition(composition: HostComposition) -> anyhow::Result<()> {
    app_api::run_from_env_with_composition(composition).await
}

/// Start the Lenso worker from environment configuration.
pub async fn run_worker_from_env() -> anyhow::Result<()> {
    run_worker_from_env_with_composition(HostComposition::default()).await
}

/// Start the Lenso worker with additional host-owned linked modules.
pub async fn run_worker_from_env_with_composition(
    composition: HostComposition,
) -> anyhow::Result<()> {
    app_worker::run_from_env_with_composition(composition).await
}

/// Apply Lenso migrations from environment configuration.
pub async fn run_migrations_from_env() -> anyhow::Result<()> {
    run_migrations_from_env_with_composition(HostComposition::default()).await
}

/// Apply Lenso migrations with additional host-owned linked module migrations.
pub async fn run_migrations_from_env_with_composition(
    composition: HostComposition,
) -> anyhow::Result<()> {
    app_migrate::run_from_env_with_composition(composition).await
}
