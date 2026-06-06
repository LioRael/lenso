//! Worker process runtime-config knobs, live-tunable via the Console.
//!
//! These are platform-runtime concerns (poll cadence, batch size), not a
//! product module, so they are registered as platform-owned descriptors at the
//! composition root rather than via a module manifest.

use crate::runtime_config::{RuntimeConfigDescriptor, RuntimeConfigScope, RuntimeConfigType};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

/// Worker config resolved from the snapshot under the `worker.` key prefix.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerRuntimeConfig {
    pub poll_interval_ms: u64,
    pub batch_size: u64,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 500,
            batch_size: 25,
        }
    }
}

/// Platform-owned, worker-scoped runtime config descriptors.
pub static RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>> = LazyLock::new(|| {
    vec![
        RuntimeConfigDescriptor {
            key: "worker.poll_interval_ms",
            scope: RuntimeConfigScope::Service("worker"),
            value_type: RuntimeConfigType::Int {
                min: Some(50),
                max: Some(60_000),
            },
            default: json!(500),
            editable: true,
            restart_only: false,
            description: "Milliseconds the worker sleeps between poll ticks.",
        },
        RuntimeConfigDescriptor {
            key: "worker.batch_size",
            scope: RuntimeConfigScope::Service("worker"),
            value_type: RuntimeConfigType::Int {
                min: Some(1),
                max: Some(1_000),
            },
            default: json!(25),
            editable: true,
            restart_only: false,
            description: "Maximum outbox events / function runs claimed per tick.",
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_config::{RuntimeConfigRegistry, RuntimeConfigSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_defaults_from_snapshot() {
        let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).unwrap();
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "worker", &BTreeMap::new());
        let cfg: WorkerRuntimeConfig = snapshot.get("worker").unwrap();
        assert_eq!(cfg.poll_interval_ms, 500);
        assert_eq!(cfg.batch_size, 25);
    }

    #[test]
    fn default_impl_matches_descriptor_defaults() {
        let defaults = WorkerRuntimeConfig::default();
        let poll = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.poll_interval_ms")
            .unwrap();
        let batch = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.batch_size")
            .unwrap();
        assert_eq!(defaults.poll_interval_ms, poll.default.as_u64().unwrap());
        assert_eq!(defaults.batch_size, batch.default.as_u64().unwrap());
    }

    #[test]
    fn poll_interval_out_of_range_rejected() {
        let poll = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.poll_interval_ms")
            .unwrap();
        assert!(poll.validate(&json!(500)).is_ok());
        assert!(poll.validate(&json!(49)).is_err());
        assert!(poll.validate(&json!(60_001)).is_err());
    }

    #[test]
    fn batch_size_out_of_range_rejected() {
        let batch = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.batch_size")
            .unwrap();
        assert!(batch.validate(&json!(25)).is_ok());
        assert!(batch.validate(&json!(0)).is_err());
        assert!(batch.validate(&json!(1001)).is_err());
    }
}
