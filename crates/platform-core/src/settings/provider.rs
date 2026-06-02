use crate::settings::descriptor::SettingsRegistry;
use crate::settings::snapshot::SettingsSnapshot;
use arc_swap::ArcSwap;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Read access to the current effective configuration for this service.
///
/// Reads are cheap and never touch the database; they read the in-memory
/// snapshot maintained by the implementation.
pub trait SettingsProvider: Debug + Send + Sync {
    /// The current effective snapshot.
    fn snapshot(&self) -> Arc<SettingsSnapshot>;
}

/// Defaults-only provider for tests, the migrate app, and any context without a
/// database-backed configuration source.
#[derive(Debug)]
pub struct StaticSettingsProvider {
    snapshot: Arc<SettingsSnapshot>,
}

impl StaticSettingsProvider {
    /// Resolve a snapshot for `service_key` from registry defaults (no overrides).
    #[must_use]
    pub fn new(registry: &SettingsRegistry, service_key: &str) -> Self {
        let snapshot = SettingsSnapshot::resolve(registry, service_key, &BTreeMap::new());
        Self { snapshot: Arc::new(snapshot) }
    }

    /// An empty provider (no registered settings) for minimal test contexts.
    #[must_use]
    pub fn empty() -> Self {
        Self { snapshot: Arc::new(SettingsSnapshot::default()) }
    }
}

impl SettingsProvider for StaticSettingsProvider {
    fn snapshot(&self) -> Arc<SettingsSnapshot> {
        Arc::clone(&self.snapshot)
    }
}

/// Shared, atomically swappable snapshot cell used by the Postgres provider and
/// its background refresh task (Task 5).
#[derive(Debug)]
pub struct SnapshotCell {
    inner: ArcSwap<SettingsSnapshot>,
}

impl SnapshotCell {
    #[must_use]
    pub fn new(initial: SettingsSnapshot) -> Self {
        Self { inner: ArcSwap::from_pointee(initial) }
    }

    #[must_use]
    pub fn load(&self) -> Arc<SettingsSnapshot> {
        self.inner.load_full()
    }

    pub fn store(&self, snapshot: SettingsSnapshot) {
        self.inner.store(Arc::new(snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::descriptor::{SettingDescriptor, SettingScope, SettingType};
    use serde_json::json;

    #[test]
    fn static_provider_serves_defaults() {
        let registry = SettingsRegistry::try_new(vec![SettingDescriptor {
            key: "demo.enabled",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(true),
            editable: true,
            restart_only: false,
            description: "flag",
        }])
        .unwrap();
        let provider = StaticSettingsProvider::new(&registry, "api");
        assert_eq!(provider.snapshot().raw("demo.enabled"), Some(&json!(true)));
    }

    #[test]
    fn snapshot_cell_swaps() {
        let cell = SnapshotCell::new(SettingsSnapshot::default());
        assert!(cell.load().raw("x").is_none());
        let registry = SettingsRegistry::try_new(vec![SettingDescriptor {
            key: "x",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(false),
            editable: true,
            restart_only: false,
            description: "x",
        }])
        .unwrap();
        cell.store(SettingsSnapshot::resolve(&registry, "api", &BTreeMap::new()));
        assert_eq!(cell.load().raw("x"), Some(&json!(false)));
    }
}
