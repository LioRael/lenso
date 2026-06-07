use crate::runtime_config::descriptor::RuntimeConfigRegistry;
use crate::runtime_config::snapshot::RuntimeConfigSnapshot;
use arc_swap::ArcSwap;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Read access to the current effective configuration for this service.
///
/// Reads are cheap and never touch the database; they read the in-memory
/// snapshot maintained by the implementation.
pub trait RuntimeConfigProvider: Debug + Send + Sync {
    /// The current effective snapshot.
    fn snapshot(&self) -> Arc<RuntimeConfigSnapshot>;
}

/// Defaults-only provider for tests, the migrate app, and any context without a
/// database-backed configuration source.
#[derive(Debug)]
pub struct StaticRuntimeConfigProvider {
    snapshot: Arc<RuntimeConfigSnapshot>,
}

impl StaticRuntimeConfigProvider {
    /// Resolve a snapshot for `service_key` from registry defaults (no overrides).
    #[must_use]
    pub fn new(registry: &RuntimeConfigRegistry, service_key: &str) -> Self {
        let snapshot = RuntimeConfigSnapshot::resolve(registry, service_key, &BTreeMap::new());
        Self {
            snapshot: Arc::new(snapshot),
        }
    }

    /// An empty provider (no registered config values) for minimal test contexts.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            snapshot: Arc::new(RuntimeConfigSnapshot::default()),
        }
    }
}

impl RuntimeConfigProvider for StaticRuntimeConfigProvider {
    fn snapshot(&self) -> Arc<RuntimeConfigSnapshot> {
        Arc::clone(&self.snapshot)
    }
}

/// Shared, atomically swappable snapshot cell used by the Postgres provider and
/// its background refresh task (Task 5).
#[derive(Debug)]
pub struct RuntimeConfigCell {
    inner: ArcSwap<RuntimeConfigSnapshot>,
}

impl RuntimeConfigCell {
    #[must_use]
    pub fn new(initial: RuntimeConfigSnapshot) -> Self {
        Self {
            inner: ArcSwap::from_pointee(initial),
        }
    }

    #[must_use]
    pub fn load(&self) -> Arc<RuntimeConfigSnapshot> {
        self.inner.load_full()
    }

    pub fn store(&self, snapshot: RuntimeConfigSnapshot) {
        self.inner.store(Arc::new(snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_config::descriptor::{
        RuntimeConfigDescriptor, RuntimeConfigScope, RuntimeConfigType,
    };
    use serde_json::json;

    #[test]
    fn static_provider_serves_defaults() {
        let registry = RuntimeConfigRegistry::try_new(vec![RuntimeConfigDescriptor {
            key: "demo.enabled".to_owned(),
            scope: RuntimeConfigScope::Shared,
            value_type: RuntimeConfigType::Bool,
            default: json!(true),
            editable: true,
            restart_only: false,
            description: "flag",
        }])
        .unwrap();
        let provider = StaticRuntimeConfigProvider::new(&registry, "api");
        assert_eq!(provider.snapshot().raw("demo.enabled"), Some(&json!(true)));
    }

    #[test]
    fn snapshot_cell_swaps() {
        let cell = RuntimeConfigCell::new(RuntimeConfigSnapshot::default());
        assert!(cell.load().raw("x").is_none());
        let registry = RuntimeConfigRegistry::try_new(vec![RuntimeConfigDescriptor {
            key: "x".to_owned(),
            scope: RuntimeConfigScope::Shared,
            value_type: RuntimeConfigType::Bool,
            default: json!(false),
            editable: true,
            restart_only: false,
            description: "x",
        }])
        .unwrap();
        cell.store(RuntimeConfigSnapshot::resolve(
            &registry,
            "api",
            &BTreeMap::new(),
        ));
        assert_eq!(cell.load().raw("x"), Some(&json!(false)));
    }
}
