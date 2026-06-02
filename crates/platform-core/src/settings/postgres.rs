use crate::db::DbPool;
use crate::error::AppResult;
use crate::settings::descriptor::SettingsRegistry;
use crate::settings::provider::{SettingsProvider, SnapshotCell};
use crate::settings::snapshot::{SettingSource, SettingsSnapshot};
use crate::settings::store::load_all_values;
use serde_json::Value;
use sqlx::postgres::PgListener;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The channel name used for cross-instance config-change notifications.
pub const CONFIG_NOTIFY_CHANNEL: &str = "config_changed";

/// Capture the startup-resolved (value, source) for every restart-only
/// descriptor applicable to this service, so later refreshes can revert them.
fn freeze_restart_only(
    registry: &SettingsRegistry,
    snapshot: &SettingsSnapshot,
) -> BTreeMap<String, (Value, SettingSource)> {
    let mut frozen = BTreeMap::new();
    for descriptor in registry.iter() {
        if !descriptor.restart_only {
            continue;
        }
        if let (Some(value), Some(source)) = (
            snapshot.raw(descriptor.key),
            snapshot.source(descriptor.key),
        ) {
            frozen.insert(descriptor.key.to_owned(), (value.clone(), source));
        }
    }
    frozen
}

/// Database-backed settings provider. Holds an atomically swappable snapshot
/// resolved from the registry plus stored overrides for one running service.
#[derive(Debug)]
pub struct PostgresSettingsProvider {
    pool: DbPool,
    registry: Arc<SettingsRegistry>,
    service_key: String,
    cell: Arc<SnapshotCell>,
    /// Restart-only keys frozen at their startup-resolved (value, source).
    /// Re-applied on every refresh so running instances keep the startup value
    /// until the process restarts.
    restart_only_frozen: BTreeMap<String, (Value, SettingSource)>,
}

impl PostgresSettingsProvider {
    /// Construct the provider and load the initial snapshot from the store.
    pub async fn connect(
        pool: DbPool,
        registry: Arc<SettingsRegistry>,
        service_key: impl Into<String>,
    ) -> AppResult<Arc<Self>> {
        let service_key = service_key.into();
        let stored = load_all_values(&pool).await?;
        let snapshot = SettingsSnapshot::resolve(&registry, &service_key, &stored);
        let restart_only_frozen = freeze_restart_only(&registry, &snapshot);
        let cell = Arc::new(SnapshotCell::new(snapshot));
        Ok(Arc::new(Self {
            pool,
            registry,
            service_key,
            cell,
            restart_only_frozen,
        }))
    }

    /// Reload all stored values and swap in a fresh snapshot.
    ///
    /// Restart-only keys are reverted to their frozen startup values so they do
    /// not take effect until the process restarts.
    pub async fn refresh(&self) -> AppResult<()> {
        let stored = load_all_values(&self.pool).await?;
        let snapshot = SettingsSnapshot::resolve(&self.registry, &self.service_key, &stored)
            .with_overrides(&self.restart_only_frozen);
        self.cell.store(snapshot);
        Ok(())
    }

    /// Spawn the background LISTEN task. Refreshes on every notification and
    /// fully reloads on (re)connect, so missed notifications self-heal.
    pub fn spawn_listener(self: &Arc<Self>) {
        let provider = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                match PgListener::connect_with(&provider.pool).await {
                    Ok(mut listener) => {
                        if let Err(error) = listener.listen(CONFIG_NOTIFY_CHANNEL).await {
                            tracing::warn!(error = ?error, "config listener failed to subscribe");
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        // Reconcile after (re)subscribing in case we missed events.
                        if let Err(error) = provider.refresh().await {
                            tracing::warn!(error = ?error, "config refresh after subscribe failed");
                        }
                        loop {
                            match listener.recv().await {
                                Ok(notification) => {
                                    tracing::debug!(
                                        payload = %notification.payload(),
                                        "config change notification received"
                                    );
                                    if let Err(error) = provider.refresh().await {
                                        tracing::warn!(error = ?error, "config refresh failed");
                                    }
                                }
                                Err(error) => {
                                    tracing::warn!(error = ?error, "config listener disconnected");
                                    break;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "config listener connect failed");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}

impl SettingsProvider for PostgresSettingsProvider {
    fn snapshot(&self) -> Arc<SettingsSnapshot> {
        self.cell.load()
    }
}
